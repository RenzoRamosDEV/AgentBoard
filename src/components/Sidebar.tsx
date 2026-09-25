import { useState } from "react";
import type { AgentRow, ProjectRow } from "../lib/api";
import { fmt } from "../lib/format";
import { PERIODS, type Period, type PeriodKind } from "../lib/period";
import { SECTIONS, type SectionId } from "../lib/sections";
import { GearIcon, PanelIcon, SearchIcon, SectionIcon } from "./Icons";
import { Select } from "./Select";
import { t } from "../lib/i18n";
import logo1x from "../assets/logo-132.png";
import logo2x from "../assets/logo-264.png";
import logo3x from "../assets/logo-396.png";
import light1x from "../assets/logo-light-132.png";
import light2x from "../assets/logo-light-264.png";
import light3x from "../assets/logo-light-396.png";

interface Props {
  section: SectionId;
  setSection: (s: SectionId) => void;
  agents: AgentRow[];
  projects: ProjectRow[];
  hiddenAgents: Set<string>;
  hiddenProjects: Set<number>;
  toggleAgent: (id: string) => void;
  toggleProject: (id: number) => void;
  onlyProject: (id: number | null) => void;
  period: Period;
  setPeriod: (p: Period) => void;
  onSettings: () => void;
}

const COLLAPSE_KEY = "agentboard.sidebarCollapsed";
const loadCollapsed = () => {
  try {
    return localStorage.getItem(COLLAPSE_KEY) === "1";
  } catch {
    return false;
  }
};

/** Panel izquierdo: apartados, filtros, datos y ajustes. Se puede colapsar a solo iconos. */
export function Sidebar(p: Props) {
  const [projectQ, setProjectQ] = useState("");
  const [collapsed, setCollapsed] = useState(loadCollapsed);
  const projects = p.projects.filter((x) => x.name.toLowerCase().includes(projectQ.trim().toLowerCase()));
  const allShown = p.hiddenProjects.size === 0;

  const toggleCollapsed = () => {
    setCollapsed((v) => {
      const next = !v;
      try {
        localStorage.setItem(COLLAPSE_KEY, next ? "1" : "0");
      } catch {
        /* almacenamiento no disponible; el estado vale para esta sesión */
      }
      return next;
    });
  };

  return (
    <aside className={`sidebar ${collapsed ? "collapsed" : ""}`}>
      <button
        type="button"
        className="sidebar-toggle"
        onClick={toggleCollapsed}
        aria-label={collapsed ? t("Expandir panel") : t("Colapsar panel")}
        title={collapsed ? t("Expandir panel") : t("Colapsar panel")}
      >
        <PanelIcon />
      </button>

      <div className="brand">
        <img className="brand-logo logo-dark" src={logo1x} srcSet={`${logo1x} 1x, ${logo2x} 2x, ${logo3x} 3x`} alt="" width={132} height={132} />
        <img className="brand-logo logo-light" src={light1x} srcSet={`${light1x} 1x, ${light2x} 2x, ${light3x} 3x`} alt="" width={132} height={132} />
        <span className="brand-name">AgentBoard</span>
      </div>

      <section className="nav-section">
        <h3>{t("Apartados")}</h3>
        <nav className="nav" aria-label="Apartados">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              className={`nav-item ${p.section === s.id ? "active" : ""}`}
              onClick={() => p.setSection(s.id)}
              title={t(s.title)}
              aria-label={t(s.title)}
            >
              <SectionIcon id={s.id} />
              <span className="nav-label">{t(s.title)}</span>
            </button>
          ))}
        </nav>
      </section>

      <section>
        <h3>{t("Periodo")}</h3>
        <Select
          ariaLabel={t("Periodo")}
          value={p.period.kind}
          options={PERIODS.map(({ kind, label }) => ({ value: kind, label: t(label) }))}
          onChange={(kind: PeriodKind) => p.setPeriod({ kind })}
        />
      </section>

      <section>
        <h3>{t("Agentes")}</h3>
        {p.agents.length === 0 && <p className="muted small">{t("Todavía no se ha detectado ningún agente.")}</p>}
        <ul className="checklist">
          {p.agents.map((a) => (
            <li key={a.id}>
              <label title={a.logRoot}>
                <input type="checkbox" checked={!p.hiddenAgents.has(a.id)} onChange={() => p.toggleAgent(a.id)} />
                <span className="name">{a.name}</span>
                <span className="muted num">{fmt.usd(a.costUsd)}</span>
              </label>
            </li>
          ))}
        </ul>
      </section>

      <section className="grow">
        <h3>
          {t("Proyectos")}
          {!allShown && (
            <button className="link" onClick={() => p.onlyProject(null)}>
              {t("Todos")}
            </button>
          )}
        </h3>
        <label className="search">
          <SearchIcon />
          <input type="search" placeholder={t("Buscar proyecto…")} value={projectQ} onChange={(e) => setProjectQ(e.target.value)} aria-label={t("Buscar proyecto")} />
        </label>
        <ul className="checklist scroll">
          {projects.map((x) => (
            <li key={x.id}>
              <label title={x.cwd}>
                <input type="checkbox" checked={!p.hiddenProjects.has(x.id)} onChange={() => p.toggleProject(x.id)} />
                <span className="name">{x.name}</span>
                <span className="muted num">{fmt.usd(x.costUsd)}</span>
              </label>
              <button className="link only" onClick={() => p.onlyProject(x.id)} title={t("Ver solo este proyecto")}>
                {t("solo")}
              </button>
            </li>
          ))}
          {!projects.length && <li className="muted small">{t("Sin proyectos")}</li>}
        </ul>
      </section>

      <section className="data-info">
        <button className="button" onClick={p.onSettings} title={t("Ajustes")} aria-label={t("Ajustes")}>
          <GearIcon /> <span className="nav-label">{t("Ajustes")}</span>
        </button>
      </section>
    </aside>
  );
}
