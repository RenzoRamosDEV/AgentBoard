import { useMemo, useState } from "react";
import { api, type Filter, type ProjectSummary } from "../lib/api";
import { filterKey, useQuery } from "../lib/query";
import { agentColor, fmt, modelName } from "../lib/format";
import { t } from "../lib/i18n";
import type { Period } from "../lib/period";
import { Segmented } from "../components/Charts";
import { DataTable, type Column } from "../components/DataTable";
import { Kpis, type Kpi } from "../components/Kpis";
import { SearchIcon } from "../components/Icons";
import { periodLabel } from "./Overview";
import { quantile } from "./Sessions";

type View = "all" | "expensive" | "compact" | "sub";

const VIEWS: { value: View; label: string }[] = [
  { value: "all", label: "Todos" },
  { value: "expensive", label: "Más caros" },
  { value: "compact", label: "Con compactación" },
  { value: "sub", label: "Con subagentes" },
];

export function projectStats(rows: ProjectSummary[]) {
  const costs = rows.map((r) => r.costUsd).sort((a, b) => a - b);
  const total = costs.reduce((a, b) => a + b, 0);
  const top = rows.reduce<ProjectSummary | null>((a, b) => (!a || b.costUsd > a.costUsd ? b : a), null);
  return {
    avg: rows.length ? total / rows.length : 0,
    p75: quantile(costs, 0.75),
    total,
    top,
    activeMs: rows.reduce((a, r) => a + r.activeMs, 0),
    withCompactions: rows.filter((r) => r.compactions > 0).length,
  };
}

export const matchesProject = (r: ProjectSummary, q: string) => {
  const needle = q.trim().toLowerCase();
  if (!needle) return true;
  return [r.name, r.path, r.model, ...r.branches, ...r.agents.map((a) => a.name)].some((v) => v?.toLowerCase().includes(needle));
};

/** Colores de los agentes de un proyecto, con sus nombres al pasar el ratón. */
export const AgentDots = ({ agents }: { agents: { id: string; name: string }[] }) => (
  <span className="agent-dots" title={agents.map((a) => a.name).join(", ")}>
    {agents.map((a) => (
      <i key={a.id} style={{ background: agentColor(a.id) }} />
    ))}
    <span>{agents.length === 1 ? agents[0].name : t("{n} agentes", { n: agents.length })}</span>
  </span>
);

/** Apartado Proyectos: cifras, búsqueda, vistas rápidas y tabla ordenable (una fila por proyecto). */
const NO_PROJECTS: ProjectSummary[] = [];

/** Columnas de la tabla de proyectos (no dependen de los datos). */
const projectColumns = (): Column<ProjectSummary>[] => [
  {
    header: t("Proyecto"),
    cell: (r) => (
      <span className="two-lines">
        <span>{r.key ? r.name : t("(sin proyecto)")}</span>
        <span className="muted mono small" title={r.path ?? ""}>
          {r.path ?? "–"}
        </span>
      </span>
    ),
    width: "minmax(170px, 1.6fr)",
    sort: (r) => r.name.toLowerCase(),
  },
  { header: t("Agentes"), cell: (r) => <AgentDots agents={r.agents} />, width: "minmax(110px, 0.9fr)", sort: (r) => r.agents.length },
  { header: t("Ramas"), cell: (r) => (r.branches.length ? fmt.int(r.branches.length) : "–"), align: "right", width: "56px", sort: (r) => r.branches.length },
  { header: t("Modelo"), cell: (r) => (r.model ? modelName(r.model) : "–"), width: "minmax(90px, 0.8fr)", className: "secondary", sort: (r) => r.model ?? "" },
  { header: t("Sesiones"), cell: (r) => fmt.int(r.sessions), align: "right", width: "64px", sort: (r) => r.sessions },
  { header: t("Tiempo activo"), cell: (r) => fmt.duration(r.activeMs), align: "right", width: "92px", sort: (r) => r.activeMs },
  { header: t("Turnos"), cell: (r) => (r.turns ? fmt.int(r.turns) : "–"), align: "right", width: "56px", sort: (r) => r.turns },
  {
    header: t("Compact."),
    cell: (r) => (r.compactions ? fmt.int(r.compactions) : "–"),
    align: "right",
    width: "64px",
    className: (r) => (r.compactions >= 3 ? "error" : r.compactions ? "" : "muted"),
    sort: (r) => r.compactions,
  },
  { header: t("Caché"), cell: (r) => (r.cacheHit ? fmt.pct(r.cacheHit) : "–"), align: "right", width: "60px", className: "secondary", sort: (r) => r.cacheHit },
  {
    header: t("Coste"),
    cell: (r) => (!r.hasPrice && r.costUsd === 0 ? t("sin precio") : fmt.usd(r.costUsd)),
    align: "right",
    width: "84px",
    className: (r) => (!r.hasPrice && r.costUsd === 0 ? "warn" : "cost"),
    sort: (r) => r.costUsd,
  },
];

export function Projects({ filter, period, open }: { filter: Filter; period: Period; open: (key: string) => void }) {
  const { data: list, error } = useQuery("list_project_summaries", filterKey(filter), () => api.projectSummaries(filter));
  const [q, setQ] = useState("");
  const [view, setView] = useState<View>("all");

  const all = list ?? NO_PROJECTS;
  const stats = useMemo(() => projectStats(all), [all]);
  const columns = useMemo(projectColumns, []);
  const rows = useMemo(
    () =>
      all.filter(
        (r) =>
          matchesProject(r, q) &&
          (view === "all" ||
            (view === "expensive" && r.costUsd > 0 && r.costUsd >= stats.p75) ||
            (view === "compact" && r.compactions > 0) ||
            (view === "sub" && r.subagentCalls > 0)),
      ),
    [all, q, view, stats],
  );
  if (error) return <div className="main error">{t("No se pudieron cargar los datos: {e}", { e: error })}</div>;
  if (!list) return <div className="main muted">{t("Cargando…")}</div>;


  const kpis: Kpi[] = [
    { label: t("Coste medio por proyecto"), value: fmt.usd(stats.avg), hint: t("{n} proyectos", { n: fmt.int(all.length) }) },
    { label: t("Proyecto más caro"), value: stats.top?.name ?? "–", hint: stats.top ? `${fmt.usd(stats.top.costUsd)} · ${fmt.pct(stats.total ? stats.top.costUsd / stats.total : 0)}` : "", tone: "accent" },
    { label: t("Tiempo activo"), value: fmt.duration(stats.activeMs), hint: t("suma de la duración de las sesiones") },
    { label: t("Con compactaciones"), value: fmt.int(stats.withCompactions), hint: t("proyectos con el contexto desbordado"), tone: stats.withCompactions ? "warn" : undefined },
  ];


  return (
    <div className="main">
      <header className="page-head">
        <h1>
          {t("Proyectos")}{" "}
          <span className="muted">
            · {t("{n} en el periodo", { n: fmt.int(all.length) })} · {periodLabel(period)}
          </span>
        </h1>
      </header>
      <Kpis items={kpis} compact />
      <div className="toolbar">
        <label className="search toolbar-search">
          <SearchIcon />
          <input type="search" placeholder={t("Proyecto, rama, modelo o agente")} value={q} onChange={(e) => setQ(e.target.value)} aria-label={t("Buscar proyectos")} />
        </label>
        <Segmented value={view} options={VIEWS.map((v) => ({ ...v, label: t(v.label) }))} onChange={setView} />
      </div>
      <section className="panel">
        <DataTable
          rows={rows}
          rowKey={(r) => r.key}
          columns={columns}
          collapse={50}
          defaultSort={{ header: t("Coste"), dir: "desc" }}
          onRowClick={(r) => open(r.key)}
          empty={all.length ? t("Ningún proyecto coincide con la búsqueda") : t("Sin actividad en este periodo")}
        />
        <footer className="panel-foot muted small">{t("Pulsa un proyecto para ver su detalle y sus sesiones")}</footer>
      </section>
    </div>
  );
}
