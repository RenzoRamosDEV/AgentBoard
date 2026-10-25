import { useEffect, useState } from "react";
import type { AgentRow, ProjectRow } from "../lib/api";
import { fmt } from "../lib/format";
import { t } from "../lib/i18n";
import { PERIODS, type Period } from "../lib/period";
import { Popover } from "./Popover";
import { DownloadIcon, SearchIcon } from "./Icons";

interface Props {
  period: Period;
  setPeriod: (p: Period) => void;
  agents: AgentRow[];
  projects: ProjectRow[];
  hiddenAgents: Set<string>;
  hiddenProjects: Set<number>;
  toggleAgent: (id: string) => void;
  toggleProject: (id: number) => void;
  onlyProject: (id: number | null) => void;
  /** Hora (epoch ms) de la última relectura de los logs. */
  lastScan: number | null;
  /** `true` justo después de una relectura en vivo. */
  flash: boolean;
  onExport: (format: "csv" | "json") => void;
  compare: boolean;
  setCompare: (v: boolean) => void;
  /** `false` con «Todo», que no tiene periodo anterior. */
  canCompare: boolean;
  onSearch: () => void;
}

/** «hace 12 s», «hace 3 min»… a partir de la hora de la última relectura. */
function useAgo(ts: number | null) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  if (ts == null) return null;
  const s = Math.max(0, Math.round((now - ts) / 1000));
  if (s < 60) return t("hace {n} s", { n: s });
  const m = Math.round(s / 60);
  if (m < 60) return t("hace {n} min", { n: m });
  return t("hace {n} h", { n: Math.round(m / 60) });
}

/** Barra superior: periodo, agentes, proyectos, estado en vivo y exportar. */
export function Topbar(p: Props) {
  const [projectQ, setProjectQ] = useState("");
  const ago = useAgo(p.lastScan);
  const shownAgents = p.agents.filter((a) => !p.hiddenAgents.has(a.id)).length;
  const shownProjects = p.projects.filter((x) => !p.hiddenProjects.has(x.id));
  const projectsLabel =
    p.hiddenProjects.size === 0
      ? t("Todos")
      : shownProjects.length === 1
        ? shownProjects[0].name
        : t("{n} de {m}", { n: shownProjects.length, m: p.projects.length });
  const projects = p.projects.filter((x) => x.name.toLowerCase().includes(projectQ.trim().toLowerCase()));

  return (
    <div className="topbar">
      <div className="segmented" role="group" aria-label={t("Periodo")}>
        {PERIODS.map((x) => (
          <button key={x.kind} type="button" data-perf={`period-${x.kind}`} className={`segment ${p.period.kind === x.kind ? "active" : ""}`} aria-pressed={p.period.kind === x.kind} onClick={() => p.setPeriod({ kind: x.kind })}>
            {t(x.label)}
          </button>
        ))}
      </div>

      <Popover
        ariaLabel={t("Agentes")}
        perf="agents"
        label={
          <>
            <span className="muted">{t("Agentes")}</span>
            <b>{p.hiddenAgents.size === 0 ? t("Todos") : t("{n} de {m}", { n: shownAgents, m: p.agents.length })}</b>
          </>
        }
      >
        {p.agents.length === 0 && <p className="muted small">{t("Todavía no se ha detectado ningún agente.")}</p>}
        <ul className="checklist">
          {p.agents.map((a) => (
            <li key={a.id}>
              <label title={a.logRoot}>
                <input type="checkbox" data-perf={`agent-${a.id}`} checked={!p.hiddenAgents.has(a.id)} onChange={() => p.toggleAgent(a.id)} />
                <span className="name">{a.name}</span>
                <span className="muted num">{fmt.usd(a.costUsd)}</span>
              </label>
            </li>
          ))}
        </ul>
      </Popover>

      <Popover
        ariaLabel={t("Proyectos")}
        label={
          <>
            <span className="muted">{t("Proyectos")}</span>
            <b className="ellipsis">{projectsLabel}</b>
          </>
        }
      >
        <div className="popover-head">
          <label className="search">
            <SearchIcon />
            <input type="search" placeholder={t("Buscar proyecto…")} value={projectQ} onChange={(e) => setProjectQ(e.target.value)} aria-label={t("Buscar proyecto")} />
          </label>
          {p.hiddenProjects.size > 0 && (
            <button className="link" onClick={() => p.onlyProject(null)}>
              {t("Todos")}
            </button>
          )}
        </div>
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
      </Popover>

      <button
        type="button"
        className="chip-button"
        role="switch"
        aria-checked={p.compare && p.canCompare}
        disabled={!p.canCompare}
        title={p.canCompare ? undefined : t("«Todo» no tiene periodo anterior")}
        onClick={() => p.setCompare(!p.compare)}
      >
        <span className={`switch ${p.compare && p.canCompare ? "on" : ""}`} aria-hidden />
        <span className="long-label">{t("Comparar con periodo anterior")}</span>
        <span className="short-label">{t("Comparar")}</span>
      </button>

      <div className="topbar-spacer" />

      <div className={`live ${p.flash ? "flash" : ""}`} role="status" aria-live="polite">
        <span className="live-dot" />
        <span>{ago ? t("En vivo · {ago}", { ago }) : t("En vivo")}</span>
      </div>

      <button type="button" className="chip-button search-button" onClick={p.onSearch} aria-label={t("Buscar")} aria-keyshortcuts="Control+K">
        <SearchIcon />
        <span className="long-label">{t("Buscar…")}</span>
        <kbd className="long-label">Ctrl K</kbd>
      </button>

      <Popover ariaLabel={t("Exportar")} align="right" chevron={false} className="icon-only" label={<DownloadIcon />}>
        {(close) => (
          <div className="menu">
            <p className="muted small">{t("Descarga los datos del periodo y los filtros actuales.")}</p>
            <button className="menu-item" onClick={() => (p.onExport("csv"), close())}>
              CSV
            </button>
            <button className="menu-item" onClick={() => (p.onExport("json"), close())}>
              JSON
            </button>
          </div>
        )}
      </Popover>
    </div>
  );
}
