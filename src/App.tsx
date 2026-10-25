import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { subscribeApp } from "./lib/live";
import { api, type AgentRow, type Filter, type ProjectRow, type Settings } from "./lib/api";
import { periodRange, previousRange, type Period } from "./lib/period";
import { applyTheme, storedTheme } from "./lib/theme";
import { LangContext, resolveLang, setLang, t, type LangSetting } from "./lib/i18n";
import { SECTIONS, type SectionId } from "./lib/sections";
import { useDashboardData } from "./lib/useData";
import { Sidebar } from "./components/Sidebar";
import { Topbar } from "./components/Topbar";
import { SettingsDialog } from "./components/SettingsDialog";
import { TooltipProvider } from "./components/Tooltip";
import { Overview } from "./views/Overview";
import { Section } from "./views/Section";
import { Projects } from "./views/Projects";
import { ProjectDetail } from "./views/ProjectDetail";
import { SessionDetail } from "./views/SessionDetail";
import { Pricing } from "./views/Pricing";
import { CommandPalette } from "./components/CommandPalette";

/** Idioma recordado en este equipo, para pintar bien antes de leer los ajustes. */
function storedLang(): LangSetting {
  try {
    const l = localStorage.getItem("language");
    if (l === "es" || l === "en" || l === "pt" || l === "fr" || l === "system") return l;
  } catch {
    // ignorar
  }
  return "system";
}

export default function App() {
  // En desarrollo, `?s=<apartado>&id=<sesión>` abre directamente esa vista (capturas de pantalla).
  const devParams = import.meta.env.DEV ? new URLSearchParams(location.search) : null;
  const [section, setSectionState] = useState<SectionId>(() => {
    const s = devParams?.get("s");
    return SECTIONS.some((x) => x.id === s) ? (s as SectionId) : "overview";
  });
  // Proyecto y sesión abiertos en el apartado Proyectos; al cambiar de apartado se vuelve al listado.
  const [projectKey, setProjectKey] = useState<string | null>(() => devParams?.get("p") ?? null);
  const [sessionId, setSessionId] = useState<string | null>(() => devParams?.get("id") ?? null);
  const setSection = (s: SectionId) => {
    setSectionState(s);
    setProjectKey(null);
    setSessionId(null);
  };
  const openSession = (id: string) => {
    setSectionState("projects");
    setSessionId(id);
  };
  const [period, setPeriod] = useState<Period>({ kind: "30d" });
  // Se guardan los *ocultos*: un agente o proyecto nuevo aparece incluido por defecto.
  const [hiddenAgents, setHiddenAgents] = useState<Set<string>>(new Set());
  const [hiddenProjects, setHiddenProjects] = useState<Set<number>>(new Set());
  const [agents, setAgents] = useState<AgentRow[]>([]);
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [settings, setSettings] = useState<Settings>({ theme: storedTheme(), language: storedLang(), monthlyBudget: null, dailyBudget: null, budgets: [], priceOverrides: [], alertAt80: true, alertAt100: true, trayShowsToday: false, memoryOnly: false });
  const lang = resolveLang(settings.language);
  setLang(lang);
  const [showSettings, setShowSettings] = useState(() => devParams?.get("settings") === "1");
  const [showPalette, setShowPalette] = useState(false);
  // Sesiones leídas en total (pie del panel lateral).
  const [totalSessions, setTotalSessions] = useState<number | null>(null);
  const [refresh, setRefresh] = useState(0);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [live, setLive] = useState(false);
  const [lastScan, setLastScan] = useState<number | null>(null);

  const range = useMemo(() => periodRange(period), [period, refresh]);
  const [compare, setCompareState] = useState(() => {
    try {
      return localStorage.getItem("agentboard.compare") !== "0";
    } catch {
      return true;
    }
  });
  const setCompare = (v: boolean) => {
    setCompareState(v);
    try {
      localStorage.setItem("agentboard.compare", v ? "1" : "0");
    } catch {
      // sin almacenamiento: vale para esta sesión
    }
  };

  const filter: Filter = useMemo(
    () => ({
      ...range,
      agents: hiddenAgents.size ? agents.filter((a) => !hiddenAgents.has(a.id)).map((a) => a.id) : undefined,
      projects: hiddenProjects.size ? projects.filter((p) => !hiddenProjects.has(p.id)).map((p) => p.id) : undefined,
    }),
    [range, hiddenAgents, hiddenProjects, agents, projects],
  );

  // Las listas del panel dependen del periodo y de los agentes, no de los proyectos.
  useEffect(() => {
    const scope: Filter = { ...range, agents: filter.agents };
    api.agents(range).then(setAgents).catch((e) => setLoadError(String(e)));
    api.projects(scope).then(setProjects).catch((e) => setLoadError(String(e)));
  }, [range, filter.agents?.join(","), refresh]);

  useEffect(() => {
    api.summary({}).then((s) => setTotalSessions(s.sessions)).catch(() => setTotalSessions(null));
  }, [refresh]);

  // Ctrl+K / Cmd+K abre el buscador.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setShowPalette((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => applyTheme(settings.theme), [settings.theme]);
  useEffect(() => {
    try {
      localStorage.setItem("language", settings.language);
    } catch {
      // sin almacenamiento
    }
  }, [settings.language]);

  useEffect(() => {
    api.settings().then(setSettings).catch((e) => setLoadError(String(e)));
    // Fin del escaneo inicial (`ingest://done`) y cada aviso de datos cambiados (`data:changed`);
    // el resalte en vivo, solo con los avisos posteriores al escaneo inicial.
    return subscribeApp(listen, {
      refresh: () => {
        setRefresh((n) => n + 1);
        setLastScan(Date.now());
      },
      live: () => {
        setLive(true);
        setTimeout(() => setLive(false), 2500);
      },
    });
  }, []);

  const toggle = <T,>(set: Set<T>, v: T) => {
    const next = new Set(set);
    next.has(v) ? next.delete(v) : next.add(v);
    return next;
  };

  const onlyProject = useCallback(
    (id: number | null) => setHiddenProjects(id === null ? new Set() : new Set(projects.filter((p) => p.id !== id).map((p) => p.id))),
    [projects],
  );

  const visibleProjects = projects.filter((p) => !hiddenProjects.has(p.id));
  const singleProject = hiddenProjects.size && visibleProjects.length === 1 ? visibleProjects[0].name : null;
  // Mismo filtro de agentes y proyectos sobre el periodo anterior (no hay con «Todo»).
  const prevFilter: Filter | null = useMemo(() => {
    const prev = compare ? previousRange(period) : null;
    return prev ? { ...filter, ...prev } : null;
  }, [compare, period, filter]);
  const { data, error } = useDashboardData(filter, singleProject, refresh, prevFilter);
  const saveSettings = async (s: Settings) => {
    setSettings(await api.saveSettings(s));
    // El backend recalcula los costes (precios, presupuestos…): invalida lo ya cargado.
    setRefresh((n) => n + 1);
  };

  const doExport = async (format: "csv" | "json") => {
    try {
      const text = await api.exportData(filter, format);
      const blob = new Blob([text], { type: format === "csv" ? "text/csv;charset=utf-8" : "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `agentboard-${new Date().toISOString().slice(0, 10)}.${format}`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      setLoadError(String(e));
    }
  };

  let content;
  if (section === "projects")
    content = sessionId ? (
      <SessionDetail id={sessionId} refresh={refresh} back={() => setSessionId(null)} backLabel={projectKey != null ? t("‹ Volver al proyecto") : t("‹ Proyectos")} />
    ) : projectKey != null ? (
      <ProjectDetail projectKey={projectKey} filter={filter} refresh={refresh} back={() => setProjectKey(null)} openSession={setSessionId} />
    ) : (
      <Projects filter={filter} period={period} refresh={refresh} open={setProjectKey} />
    );
  else if (section === "pricing")
    content = <Pricing filter={filter} refresh={refresh} settings={settings} onSave={saveSettings} agents={agents} />;
  else if (error) content = <div className="main error">{t("No se pudieron cargar los datos: {e}", { e: error })}</div>;
  else if (!data) content = <div className="main muted">{t("Cargando…")}</div>;
  else if (section === "overview")
    content = <Overview data={data} period={period} budget={settings.monthlyBudget} singleProject={singleProject} open={setSection} openSession={openSession} refresh={refresh} />;
  else if (!SECTIONS.some((x) => x.id === section))
    // Apartado que ya no existe (p. ej. tras una actualización en caliente): al resumen.
    content = <Overview data={data} period={period} budget={settings.monthlyBudget} singleProject={singleProject} open={setSection} openSession={openSession} refresh={refresh} />;
  else content = <Section id={section} data={data} period={period} singleProject={singleProject} back={() => setSection("overview")} />;

  return (
    <LangContext.Provider value={lang}>
    <TooltipProvider>
      {/* Al cambiar de idioma se vuelve a montar todo con las cadenas nuevas. */}
      <div className="app" key={lang}>
        <Sidebar section={section} setSection={setSection} onSettings={() => setShowSettings(true)} agents={agents.length} sessions={totalSessions} />
        <div className="content">
          {loadError && (
            <div className="notice warn banner">
              {t("No se pudieron cargar los datos: {e}", { e: loadError })}
              <button className="link" onClick={() => setLoadError(null)}>{t("Cerrar")}</button>
            </div>
          )}
          <Topbar
            period={period}
            setPeriod={setPeriod}
            agents={agents}
            projects={projects}
            hiddenAgents={hiddenAgents}
            hiddenProjects={hiddenProjects}
            toggleAgent={(id) => setHiddenAgents((s) => toggle(s, id))}
            toggleProject={(id) => setHiddenProjects((s) => toggle(s, id))}
            onlyProject={onlyProject}
            lastScan={lastScan}
            flash={live}
            compare={compare}
            setCompare={setCompare}
            canCompare={previousRange(period) != null}
            onSearch={() => setShowPalette(true)}
            onExport={doExport}
          />
          {content}
        </div>
      </div>
      {showPalette && (
        <CommandPalette
          filter={filter}
          projects={projects}
          onClose={() => setShowPalette(false)}
          goSection={setSection}
          onlyProject={(id) => onlyProject(id)}
          openSession={(id) => {
            setProjectKey(null);
            openSession(id);
          }}
          openSettings={() => setShowSettings(true)}
        />
      )}
      {showSettings && <SettingsDialog settings={settings} onSave={saveSettings} onClose={() => setShowSettings(false)} onExport={doExport} />}
    </TooltipProvider>
    </LangContext.Provider>
  );
}
