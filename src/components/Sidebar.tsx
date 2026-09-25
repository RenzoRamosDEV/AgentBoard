import { useState } from "react";
import type { AgentRow, DataInfo, ProjectRow } from "../lib/api";
import { fmt } from "../lib/format";
import { PERIODS, toInputDate, type Period } from "../lib/period";

interface Props {
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

export function Sidebar(p: Props) {
  const [agentQ, setAgentQ] = useState("");
  const [projectQ, setProjectQ] = useState("");
  const match = (q: string) => (name: string) => name.toLowerCase().includes(q.trim().toLowerCase());
  const agents = p.agents.filter((a) => match(agentQ)(a.name));
  const projects = p.projects.filter((x) => match(projectQ)(x.name));
  const allShown = p.hiddenProjects.size === 0;

  return (
    <aside className="sidebar">
      <div className="brand">
        AgentBurn
      </div>

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
        {p.agents.length > 4 && <input className="search" placeholder="Buscar agente…" value={agentQ} onChange={(e) => setAgentQ(e.target.value)} />}
        {p.agents.length === 0 && <p className="muted small">No se encontró ningún agente todavía.</p>}
        <ul className="checklist">
          {agents.map((a) => (
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
        <input className="search" placeholder="Buscar proyecto…" value={projectQ} onChange={(e) => setProjectQ(e.target.value)} />
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
            <dt>Tamaño de la base</dt>
            <dd>{fmt.bytes(p.info.dbBytes)}</dd>
            <dt>Archivos vigilados</dt>
            <dd>{fmt.int(p.info.watchedFiles)}</dd>
          </dl>
        )}
        <button className="button" onClick={p.onSettings}>
          Ajustes
        </button>
      </section>
    </aside>
  );
}
