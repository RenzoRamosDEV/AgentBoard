import { memo, useState } from "react";
import { api } from "../lib/api";
import { useLists } from "../lib/filterStore";
import { useQuery } from "../lib/query";
import { fmt } from "../lib/format";
import { GROUPS, SECTIONS, type SectionId } from "../lib/sections";
import { GearIcon, LockIcon, PanelIcon, SectionIcon } from "./Icons";
import { t } from "../lib/i18n";
import logo1x from "../assets/brand-132.png";
import logo2x from "../assets/brand-264.png";
import light1x from "../assets/brand-light-132.png";
import light2x from "../assets/brand-light-264.png";

interface Props {
  section: SectionId;
  setSection: (s: SectionId) => void;
  onSettings: () => void;
}

const COLLAPSE_KEY = "agentboard.sidebarCollapsed";
const loadFlag = (key: string, def: boolean) => {
  try {
    const v = localStorage.getItem(key);
    return v === null ? def : v === "1";
  } catch {
    return def;
  }
};
const saveFlag = (key: string, value: boolean) => {
  try {
    localStorage.setItem(key, value ? "1" : "0");
  } catch {
    /* almacenamiento no disponible; el estado vale para esta sesión */
  }
};

/** Agentes detectados y sesiones leídas en total, para el pie. */
function useFooter() {
  const { agents } = useLists();
  const total = useQuery("get_summary", "total", () => api.summary({}));
  return { agents: agents.length, sessions: total.error ? null : (total.data?.sessions ?? null) };
}

/** Panel izquierdo: logo, apartados agrupados y ajustes. Se puede colapsar a solo iconos. */
export const Sidebar = memo(function Sidebar(p: Props) {
  const footer = useFooter();
  const [collapsed, setCollapsed] = useState(() => loadFlag(COLLAPSE_KEY, false));

  const toggleCollapsed = () => {
    setCollapsed((v) => {
      saveFlag(COLLAPSE_KEY, !v);
      return !v;
    });
  };

  return (
    <aside className={`sidebar ${collapsed ? "collapsed" : ""}`}>
      <div className="brand">
        <img className="brand-logo logo-dark" src={logo1x} srcSet={`${logo1x} 1x, ${logo2x} 2x`} alt="" width={34} height={34} />
        <img className="brand-logo logo-light" src={light1x} srcSet={`${light1x} 1x, ${light2x} 2x`} alt="" width={34} height={34} />
        <span className="brand-name">AgentBoard</span>
        <span className="brand-version">V2</span>
        <button
          type="button"
          className="sidebar-toggle"
          onClick={toggleCollapsed}
          aria-label={collapsed ? t("Expandir panel") : t("Colapsar panel")}
          title={collapsed ? t("Expandir panel") : t("Colapsar panel")}
        >
          <PanelIcon />
        </button>
      </div>

      <nav className="nav" aria-label={t("Apartados")}>
        {GROUPS.map((g) => (
          <div className="nav-group" key={g.id}>
            <h3>{t(g.title)}</h3>
            {SECTIONS.filter((s) => s.group === g.id).map((s) => (
              <button
                key={s.id}
                data-perf={`nav-${s.id}`}
                className={`nav-item ${p.section === s.id ? "active" : ""}`}
                onClick={() => p.setSection(s.id)}
                title={t(s.title)}
                aria-label={t(s.title)}
                aria-current={p.section === s.id ? "page" : undefined}
              >
                <SectionIcon id={s.id} />
                <span className="nav-label">{t(s.nav ?? s.title)}</span>
                {s.isNew && <span className="nav-new">{t("nuevo")}</span>}
              </button>
            ))}
            {g.id === "configuracion" && (
              <button className="nav-item" onClick={p.onSettings} title={t("Ajustes")} aria-label={t("Ajustes")}>
                <GearIcon />
                <span className="nav-label">{t("Ajustes")}</span>
              </button>
            )}
          </div>
        ))}
      </nav>

      <div className="sidebar-foot" title={t("Los datos se leen de los logs y viven en memoria; nada sale del equipo.")}>
        <div className="local-note">
          <LockIcon />
          <span className="nav-label">{t("Local · en memoria")}</span>
        </div>
        <div className="foot-stats nav-label num">
          {footer.sessions == null
            ? t("{n} agentes", { n: footer.agents })
            : t("{n} agentes · {m} sesiones leídas", { n: footer.agents, m: fmt.int(footer.sessions) })}
        </div>
      </div>
    </aside>
  );
});
