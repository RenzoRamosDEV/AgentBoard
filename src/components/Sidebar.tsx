import { useState } from "react";
import type { AgentRow, DataInfo, ProjectRow } from "../lib/api";
import { fmt } from "../lib/format";
import { PERIODS, toInputDate, type Period } from "../lib/period";
import { SECTIONS, type SectionId } from "../lib/sections";
import { GearIcon, LogoIcon, SearchIcon, SectionIcon } from "./Icons";

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
  info: DataInfo | null;
  onSettings: () => void;
}

/** Panel izquierdo: apartados, filtros, datos y ajustes. */
export function Sidebar(p: Props) {
  const [projectQ, setProjectQ] = useState("");
  const projects = p.projects.filter((x) => x.name.toLowerCase().includes(projectQ.trim().toLowerCase()));
  const allShown = p.hiddenProjects.size === 0;

  return (
    <aside className="sidebar">
      <div className="brand">
        <span className="brand-mark">
          <LogoIcon />
        </span>
        AgentBoard
      </div>

      <section>
        <h3>Apartados</h3>
        <nav className="nav" aria-label="Apartados">
          {SECTIONS.map((s) => (
            <button key={s.id} className={`nav-item ${p.section === s.id ? "active" : ""}`} onClick={() => p.setSection(s.id)}>
              <SectionIcon id={s.id} />
              {s.title}
            </button>
          ))}
        </nav>
      </section>

      <section>
        <h3>Periodo</h3>
        <div className="chips">
          {PERIODS.map(({ kind, label }) => (
            <button
              key={kind}
              className={`chip ${p.period.kind === kind ? "active" : ""}`}
              onClick={() =>
                p.setPeriod(
                  kind === "custom"
                    ? { kind, start: p.period.start ?? toInputDate(new Date(Date.now() - 6 * 864e5)), end: p.period.end ?? toInputDate(new Date()) }
                    : { kind },
                )
              }
            >
              {label}
            </button>
          ))}
        </div>
        {p.period.kind === "custom" && (
          <div className="range">
            <input type="date" value={p.period.start} onChange={(e) => p.setPeriod({ ...p.period, start: e.target.value })} aria-label="Desde" />
            <span className="muted">a</span>
            <input type="date" value={p.period.end} onChange={(e) => p.setPeriod({ ...p.period, end: e.target.value })} aria-label="Hasta" />
          </div>
        )}
      </section>

      <section>
        <h3>Agentes</h3>
        {p.agents.length === 0 && <p className="muted small">Todavía no se ha detectado ningún agente.</p>}
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
          Proyectos
          {!allShown && (
            <button className="link" onClick={() => p.onlyProject(null)}>
              Todos
            </button>
          )}
        </h3>
        <label className="search">
          <SearchIcon />
          <input type="search" placeholder="Buscar proyecto…" value={projectQ} onChange={(e) => setProjectQ(e.target.value)} aria-label="Buscar proyecto" />
        </label>
        <ul className="checklist scroll">
          {projects.map((x) => (
            <li key={x.id}>
              <label title={x.cwd}>
                <input type="checkbox" checked={!p.hiddenProjects.has(x.id)} onChange={() => p.toggleProject(x.id)} />
                <span className="name">{x.name}</span>
                <span className="muted num">{fmt.usd(x.costUsd)}</span>
              </label>
              <button className="link only" onClick={() => p.onlyProject(x.id)} title="Ver solo este proyecto">
                solo
              </button>
            </li>
          ))}
          {!projects.length && <li className="muted small">Sin proyectos</li>}
        </ul>
      </section>

      <section className="data-info">
        <h3>Datos</h3>
        {p.info && (
          <dl>
            <dt>Primer registro</dt>
            <dd>{p.info.firstTs ? fmt.date(p.info.firstTs) : "—"}</dd>
            <dt>Base de datos</dt>
            <dd>{fmt.bytes(p.info.dbBytes)}</dd>
            <dt>Archivos vigilados</dt>
            <dd>{fmt.int(p.info.watchedFiles)}</dd>
          </dl>
        )}
        <button className="button" onClick={p.onSettings}>
          <GearIcon /> Ajustes
        </button>
      </section>
    </aside>
  );
}
