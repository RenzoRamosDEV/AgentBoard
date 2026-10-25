import { useEffect, useMemo, useState } from "react";
import { api, type BreakdownRow, type Filter, type ProjectDetail as Detail, type SessionRow, type SessionSort, type ToolStat } from "../lib/api";
import { createPager, type PagerState } from "../lib/pager";
import { filterKey, useGeneration, useQuery } from "../lib/query";
import { activityColor, activityLabel, fmt, modelName } from "../lib/format";
import { t } from "../lib/i18n";
import { Bars, LineChart } from "../components/Charts";
import { DataTable, type Column, type SortState } from "../components/DataTable";
import { Kpis, type Kpi } from "../components/Kpis";
import { dayPoints } from "./panels";
import { AgentDots } from "./Projects";
import { sessionColumns } from "./Sessions";

/** Columnas de herramientas con latencia (compartidas con el detalle de sesión). */
export function toolColumns(): Column<ToolStat>[] {
  return [
    { header: t("Herramienta"), cell: (r) => <span title={r.tool}>{r.tool}</span>, width: "minmax(110px, 1fr)", className: "mono", sort: (r) => r.tool },
    { header: t("Usos"), cell: (r) => fmt.int(r.calls), align: "right", width: "52px", sort: (r) => r.calls },
    { header: t("Error"), cell: (r) => (r.errors ? fmt.pct(r.errors / r.calls) : "–"), align: "right", width: "56px", className: (r) => (r.errors ? "warn" : "muted"), sort: (r) => r.errors / r.calls },
    { header: "p50", cell: (r) => (r.p50Ms == null ? "–" : fmt.ms(r.p50Ms)), align: "right", width: "64px", className: "secondary", sort: (r) => r.p50Ms ?? -1 },
    { header: "p95", cell: (r) => (r.p95Ms == null ? "–" : fmt.ms(r.p95Ms)), align: "right", width: "72px", className: (r) => ((r.p95Ms ?? 0) >= 30_000 ? "warn" : "secondary"), sort: (r) => r.p95Ms ?? -1 },
  ];
}

/** Sesiones del proyecto por página. */
const SESSIONS_PAGE = 25;

/** Columna del servidor de cada cabecera ordenable de la tabla de sesiones. */
const serverSort = (header: string): SessionSort =>
  (
    ({
      [t("Agente")]: "agent",
      [t("Rama")]: "branch",
      [t("Modelo")]: "model",
      [t("Duración")]: "duration",
      [t("Turnos")]: "turns",
      [t("Compact.")]: "compactions",
      [t("Coste")]: "cost",
    }) as Record<string, SessionSort>
  )[header] ?? "started";

export function ProjectDetail({
  projectKey,
  filter,
  back,
  openSession,
}: {
  projectKey: string;
  filter: Filter;
  back: () => void;
  openSession: (id: string) => void;
}) {
  const fk = filterKey(filter);
  const { data: d, error } = useQuery<Detail>("get_project_detail", `${projectKey}|${fk}`, () => api.projectDetail(projectKey, filter));
  const generation = useGeneration();
  const [sort, setSort] = useState<SortState>({ header: t("Inicio"), dir: "desc" });
  const [sessions, setSessions] = useState<PagerState<SessionRow>>({ rows: [], total: 0 });
  // Sesiones por páginas, ordenadas en el servidor; otro orden o un aviso de datos cambiados
  // vuelven a la primera página.
  const pager = useMemo(
    () =>
      createPager<SessionRow, SortState>(
        (offset, limit, s) => api.sessions(filter, { offset, limit, sort: serverSort(s.header), desc: s.dir === "desc" }, projectKey),
        SESSIONS_PAGE,
        setSessions,
      ),
    [projectKey, fk],
  );

  useEffect(() => pager.load(sort), [pager, sort, generation]);
  const sessionCols = useMemo(() => sessionColumns(sessions.rows), [sessions.rows]);

  if (error)
    return (
      <div className="main">
        <button className="link back" onClick={back}>
          {t("‹ Proyectos")}
        </button>
        <div className="notice">{t("Este proyecto no tiene actividad en el periodo elegido.")}</div>
      </div>
    );
  if (!d) return <div className="main muted">{t("Cargando…")}</div>;

  const p = d.project;
  const withCompactions = d.sessionsWithCompactions;
  const kpis: Kpi[] = [
    { label: t("Coste"), value: p.hasPrice || p.costUsd > 0 ? fmt.usd(p.costUsd) : t("sin precio"), hint: t("{n} llamadas", { n: fmt.int(p.calls) }), tone: "accent" },
    { label: t("Sesiones"), value: fmt.int(p.sessions), hint: p.sessions ? t("{v} por sesión", { v: fmt.usd(p.costUsd / p.sessions) }) : "" },
    { label: t("Turnos"), value: p.turns ? fmt.int(p.turns) : "–", hint: p.turns ? t("{v} por turno", { v: fmt.usd(p.costUsd / p.turns) }) : "" },
    { label: t("Tiempo activo"), value: fmt.duration(p.activeMs), hint: `${fmt.day(p.firstTs)} → ${fmt.day(p.lastTs)}` },
    { label: t("Cache hit"), value: fmt.pct(p.cacheHit), hint: p.toolCalls ? t("{n} usos de herramientas", { n: fmt.int(p.toolCalls) }) : "" },
    { label: t("Compactaciones"), value: fmt.int(p.compactions), hint: withCompactions ? t("en {n} sesiones", { n: withCompactions }) : t("el contexto no llegó a llenarse"), tone: p.compactions >= 3 ? "warn" : undefined },
  ];

  // Coste acumulado por día del periodo.
  let acc = 0;
  const cumulative = dayPoints(d.daily, filter).map((x) => {
    acc += x.value;
    return {
      ts: x.ts,
      value: acc,
      tooltip: (
        <>
          <b>{x.monthly ? fmt.monthYear(x.ts) : fmt.date(x.ts)}</b>
          <div>{t("Acumulado: {v}", { v: fmt.usd(acc) })}</div>
          <div className="muted">{t(x.monthly ? "Ese mes: {v}" : "Ese día: {v}", { v: fmt.usd(x.value) })}</div>
        </>
      ),
    };
  });

  const small: Column<BreakdownRow>[] = [
    { header: "", cell: (r) => r.label, className: "secondary" },
    { header: t("Llamadas"), cell: (r) => fmt.int(r.calls), align: "right", width: "64px" },
    { header: t("Coste"), cell: (r) => fmt.usd(r.costUsd), align: "right", width: "76px", className: "cost" },
  ];
  const modelCols = [{ ...small[0], header: t("Modelo"), cell: (r: BreakdownRow) => modelName(r.key) }, ...small.slice(1)];
  const branchCols = [{ ...small[0], header: t("Rama"), cell: (r: BreakdownRow) => r.label || t("(sin rama)"), className: "mono small" }, ...small.slice(1)];

  return (
    <div className="main">
      <header className="page-head">
        <button className="link back" onClick={back}>
          {t("‹ Proyectos")}
        </button>
        <div className="session-title">
          <h1>{p.key ? p.name : t("(sin proyecto)")}</h1>
          <AgentDots agents={p.agents} />
          {p.model && <span className="tag mono">{p.model}</span>}
          <span className="muted mono session-when">{p.path}</span>
        </div>
      </header>
      <Kpis items={kpis} />
      <div className="grid-top">
        <section className="panel">
          <header className="panel-head">
            <div className="panel-title">
              <h2>{t("Coste acumulado")}</h2>
              <span className="muted">{t("por día del periodo")}</span>
            </div>
          </header>
          <LineChart points={cumulative} format={fmt.usd} height={230} />
        </section>
        <section className="panel">
          <header className="panel-head">
            <div className="panel-title">
              <h2>{t("Coste por actividad")}</h2>
            </div>
          </header>
          <Bars
            items={d.activities.map((a) => ({ key: a.key, label: activityLabel(a.key), value: a.costUsd, valueLabel: fmt.usd(a.costUsd), color: activityColor(a.key) }))}
            labelWidth={96}
            thick
            limit={6}
          />
          <DataTable rows={d.models} rowKey={(r) => r.key} columns={modelCols} limit={4} />
        </section>
      </div>
      <section className="panel">
          <header className="panel-head">
            <div className="panel-title">
              <h2>{t("Sesiones")}</h2>
              <span className="muted">{t("{n} en el periodo · pulsa una para ver su detalle", { n: fmt.int(sessions.total) })}</span>
            </div>
          </header>
          <DataTable
            rows={sessions.rows}
            rowKey={(r) => r.id}
            columns={sessionCols}
            sort={sort}
            onSortChange={setSort}
            more={{ total: sessions.total, onMore: pager.more }}
            onRowClick={(r) => openSession(r.id)}
          />
      </section>
      <div className="grid-2 grid-start">
          <section className="panel">
            <header className="panel-head">
              <div className="panel-title">
                <h2>{t("Ramas")}</h2>
              </div>
            </header>
            <DataTable rows={d.branches} rowKey={(r) => r.key} columns={branchCols} collapse={8} empty={t("Sin ramas de git")} />
          </section>
          <section className="panel">
            <header className="panel-head">
              <div className="panel-title">
                <h2>{t("Herramientas y latencia")}</h2>
              </div>
            </header>
            <DataTable rows={d.tools} rowKey={(r) => r.tool} columns={toolColumns()} collapse={12} empty={t("Sin herramientas en este proyecto")} />
          </section>
      </div>
    </div>
  );
}
