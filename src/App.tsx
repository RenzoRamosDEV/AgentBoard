import { memo, startTransition, useCallback, useDeferredValue, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { subscribeApp } from "./lib/live";
import { queryStore } from "./lib/query";
import { api, type Settings } from "./lib/api";
import { filterStore, useFilter, useLists } from "./lib/filterStore";
import { applyTheme, storedTheme } from "./lib/theme";
import { LangContext, resolveLang, setLang, t, type LangSetting } from "./lib/i18n";
import { SECTIONS, type SectionId } from "./lib/sections";
import { useDashboardData } from "./lib/useData";
import { Sidebar } from "./components/Sidebar";
import { Topbar } from "./components/Topbar";
import { SettingsDialog } from "./components/SettingsDialog";
import { TooltipProvider } from "./components/Tooltip";
import { Overview as OverviewView } from "./views/Overview";
import { Section as SectionView } from "./views/Section";
import { Projects as ProjectsView } from "./views/Projects";
import { ProjectDetail } from "./views/ProjectDetail";
import { SessionDetail } from "./views/SessionDetail";
import { Pricing } from "./views/Pricing";
import { CommandPalette } from "./components/CommandPalette";

// Memoizadas: el render urgente de un cambio de filtro (aún con el filtro anterior) no las repinta.
const Overview = memo(OverviewView);
const Section = memo(SectionView);
const Projects = memo(ProjectsView);

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
  // El cambio de apartado es una transición: el panel lateral responde y el contenido llega detrás.
  const setSection = useCallback((s: SectionId) => {
    startTransition(() => {
      setSectionState(s);
      setProjectKey(null);
      setSessionId(null);
    });
  }, []);
  const openSession = useCallback((id: string) => {
    setSectionState("projects");
    setSessionId(id);
  }, []);
  const [settings, setSettings] = useState<Settings>({ theme: storedTheme(), language: storedLang(), monthlyBudget: null, dailyBudget: null, budgets: [], priceOverrides: [], alertAt80: true, alertAt100: true, trayShowsToday: false, memoryOnly: false });
  const lang = resolveLang(settings.language);
  setLang(lang);
  const [showSettings, setShowSettings] = useState(() => devParams?.get("settings") === "1");
  const [showPalette, setShowPalette] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [live, setLive] = useState(false);
  const [lastScan, setLastScan] = useState<number | null>(null);

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
    return subscribeApp(listen, queryStore, {
      changed: () => setLastScan(Date.now()),
      live: () => {
        setLive(true);
        setTimeout(() => setLive(false), 2500);
      },
    });
  }, []);

  const saveSettings = useCallback(async (s: Settings) => {
    setSettings(await api.saveSettings(s));
    // El backend recalcula los costes (precios, presupuestos…): invalida lo ya cargado.
    queryStore.invalidateAll();
  }, []);

  // Lee el filtro al exportar, sin suscribir la app a sus cambios.
  const doExport = useCallback(async (format: "csv" | "json") => {
    try {
      const text = await api.exportData(filterStore.current()?.filter ?? {}, format);
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
  }, []);
  const openSettings = useCallback(() => setShowSettings(true), []);
  const openPalette = useCallback(() => setShowPalette(true), []);

  return (
    <LangContext.Provider value={lang}>
    <TooltipProvider>
      {/* Al cambiar de idioma se vuelve a montar todo con las cadenas nuevas. */}
      <div className="app" key={lang}>
        <Sidebar section={section} setSection={setSection} onSettings={openSettings} />
        <div className="content">
          <ListErrors onError={setLoadError} />
          {loadError && (
            <div className="notice warn banner">
              {t("No se pudieron cargar los datos: {e}", { e: loadError })}
              <button className="link" onClick={() => setLoadError(null)}>{t("Cerrar")}</button>
            </div>
          )}
          <Topbar lastScan={lastScan} flash={live} onSearch={openPalette} onExport={doExport} />
          <Content
            section={section}
            projectKey={projectKey}
            sessionId={sessionId}
            setSection={setSection}
            setProjectKey={setProjectKey}
            setSessionId={setSessionId}
            openSession={openSession}
            settings={settings}
            saveSettings={saveSettings}
          />
        </div>
      </div>
      {showPalette && (
        <PaletteHost
          onClose={() => setShowPalette(false)}
          goSection={setSection}
          openSession={(id) => {
            setProjectKey(null);
            openSession(id);
          }}
          openSettings={openSettings}
        />
      )}
      {showSettings && <SettingsDialog settings={settings} onSave={saveSettings} onClose={() => setShowSettings(false)} onExport={doExport} />}
    </TooltipProvider>
    </LangContext.Provider>
  );
}

/** Errores al cargar las listas de agentes y proyectos: al aviso de la barra. */
function ListErrors({ onError }: { onError: (e: string) => void }) {
  const { error } = useLists();
  useEffect(() => {
    if (error) onError(error);
  }, [error, onError]);
  return null;
}

/** El apartado abierto, con el filtro diferido: la barra responde al instante y el contenido se pinta detrás. */
function Content(p: {
  section: SectionId;
  projectKey: string | null;
  sessionId: string | null;
  setSection: (s: SectionId) => void;
  setProjectKey: (k: string | null) => void;
  setSessionId: (id: string | null) => void;
  openSession: (id: string) => void;
  settings: Settings;
  saveSettings: (s: Settings) => Promise<void>;
}) {
  const now = useFilter();
  const f = useDeferredValue(now);
  // El Resumen del filtro nuevo se pide ya, en el render urgente; se pinta con el diferido.
  useDashboardData(now.filter, now.singleProject, now.prevFilter);
  const { section, projectKey, sessionId, setSection, setProjectKey, setSessionId, settings } = p;
  const { data, error } = useDashboardData(f.filter, f.singleProject, f.prevFilter);
  const backToOverview = useCallback(() => setSection("overview"), [setSection]);
  const closeSession = useCallback(() => setSessionId(null), [setSessionId]);
  const closeProject = useCallback(() => setProjectKey(null), [setProjectKey]);

  if (section === "projects")
    return sessionId ? (
      <SessionDetail id={sessionId} back={closeSession} backLabel={projectKey != null ? t("‹ Volver al proyecto") : t("‹ Proyectos")} />
    ) : projectKey != null ? (
      <ProjectDetail projectKey={projectKey} filter={f.filter} back={closeProject} openSession={setSessionId} />
    ) : (
      <Projects filter={f.filter} period={f.period} open={setProjectKey} />
    );
  if (section === "pricing") return <Pricing filter={f.filter} settings={settings} onSave={p.saveSettings} agents={f.agents} />;
  if (error) return <div className="main error">{t("No se pudieron cargar los datos: {e}", { e: error })}</div>;
  if (!data) return <div className="main muted">{t("Cargando…")}</div>;
  // Un apartado que ya no existe (p. ej. tras una actualización en caliente) vuelve al resumen.
  if (section === "overview" || !SECTIONS.some((x) => x.id === section))
    return <Overview data={data} period={f.period} budget={settings.monthlyBudget} singleProject={f.singleProject} open={setSection} openSession={p.openSession} />;
  return <Section id={section} data={data} period={f.period} singleProject={f.singleProject} back={backToOverview} />;
}

/** El buscador con el filtro y los proyectos actuales. */
function PaletteHost(p: { onClose: () => void; goSection: (s: SectionId) => void; openSession: (id: string) => void; openSettings: () => void }) {
  const f = useFilter();
  return (
    <CommandPalette
      filter={f.filter}
      projects={f.projects}
      onClose={p.onClose}
      goSection={p.goSection}
      onlyProject={(id) => filterStore.onlyProject(id, f.projects.map((x) => x.id))}
      openSession={p.openSession}
      openSettings={p.openSettings}
    />
  );
}
