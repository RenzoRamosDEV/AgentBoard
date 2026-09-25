import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type AgentRow, type Filter, type ProjectRow, type Settings } from "./lib/api";
import { periodRange, type Period } from "./lib/period";
import { applyTheme, storedTheme } from "./lib/theme";
import { LangContext, resolveLang, setLang, t, type LangSetting } from "./lib/i18n";
import type { SectionId } from "./lib/sections";
import { useDashboardData } from "./lib/useData";
import { Sidebar } from "./components/Sidebar";
import { SettingsDialog } from "./components/SettingsDialog";
import { TooltipProvider } from "./components/Tooltip";
import { Overview } from "./views/Overview";
import { Section } from "./views/Section";

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
  const [section, setSection] = useState<SectionId>("overview");
  const [period, setPeriod] = useState<Period>({ kind: "30d" });
  // Se guardan los *ocultos*: un agente o proyecto nuevo aparece incluido por defecto.
  const [hiddenAgents, setHiddenAgents] = useState<Set<string>>(new Set());
  const [hiddenProjects, setHiddenProjects] = useState<Set<number>>(new Set());
  const [agents, setAgents] = useState<AgentRow[]>([]);
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [settings, setSettings] = useState<Settings>({ theme: storedTheme(), language: storedLang(), monthlyBudget: null });
  const lang = resolveLang(settings.language);
  setLang(lang);
  const [showSettings, setShowSettings] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [live, setLive] = useState(false);

  const range = useMemo(() => periodRange(period), [period, refresh]);

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
    let first = true;
    const off = listen("ingest://done", () => {
      setRefresh((n) => n + 1);
      // El primer evento es el escaneo inicial; los siguientes son relecturas en vivo.
      if (!first) {
        setLive(true);
        setTimeout(() => setLive(false), 2500);
      }
      first = false;
    });
    return () => {
      off.then((f) => f());
    };
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
  const { data, error } = useDashboardData(filter, singleProject, refresh);
  const saveSettings = async (s: Settings) => setSettings(await api.saveSettings(s));

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
  if (error) content = <div className="main error">{t("No se pudieron cargar los datos: {e}", { e: error })}</div>;
  else if (!data) content = <div className="main muted">{t("Cargando…")}</div>;
  else if (section === "overview")
    content = <Overview data={data} period={period} budget={settings.monthlyBudget} singleProject={singleProject} open={setSection} />;
  else content = <Section id={section} data={data} period={period} singleProject={singleProject} back={() => setSection("overview")} />;

  return (
    <LangContext.Provider value={lang}>
    <TooltipProvider>
      {/* Al cambiar de idioma se vuelve a montar todo con las cadenas nuevas. */}
      <div className="app" key={lang}>
        <Sidebar
          section={section}
          setSection={setSection}
          agents={agents}
          projects={projects}
          hiddenAgents={hiddenAgents}
          hiddenProjects={hiddenProjects}
          toggleAgent={(id) => setHiddenAgents((s) => toggle(s, id))}
          toggleProject={(id) => setHiddenProjects((s) => toggle(s, id))}
          onlyProject={onlyProject}
          period={period}
          setPeriod={setPeriod}
          onSettings={() => setShowSettings(true)}
          onExport={doExport}
        />
        <div className="content">
          {loadError && (
            <div className="notice warn banner">
              {t("No se pudieron cargar los datos: {e}", { e: loadError })}
              <button className="link" onClick={() => setLoadError(null)}>{t("Cerrar")}</button>
            </div>
          )}
          {live && <div className="live-badge">{t("Actualizado en vivo")}</div>}
          {content}
        </div>
      </div>
      {showSettings && <SettingsDialog settings={settings} onSave={saveSettings} onClose={() => setShowSettings(false)} />}
    </TooltipProvider>
    </LangContext.Provider>
  );
}
