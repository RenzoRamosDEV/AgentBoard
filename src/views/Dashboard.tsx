import { useEffect, useState } from "react";
import { api, type BreakdownRow, type Filter, type Point, type Summary } from "../lib/api";
import { ACTIVITY_LABELS, fmt } from "../lib/format";
import { monthStart } from "../lib/period";
import { BarList, type BarItem } from "../components/BarList";
import { Kpis } from "../components/Kpis";
import { ModelsTable } from "../components/ModelsTable";
import { MonthSpend } from "../components/MonthSpend";
import { Panel } from "../components/Panel";

interface Data {
  summary: Summary;
  month: Point[];
  projects: BreakdownRow[];
  branches: BreakdownRow[] | null;
  models: BreakdownRow[];
  activity: BreakdownRow[];
  tools: BreakdownRow[];
  commands: BreakdownRow[];
}

const costItems = (rows: BreakdownRow[], label = (r: BreakdownRow) => r.label): BarItem[] =>
  rows.map((r) => ({
    key: r.key,
    label: label(r),
    value: r.costUsd,
    valueLabel: fmt.usd(r.costUsd),
    tooltip: (
      <>
        <b>{label(r)}</b>
        <div>{fmt.usd(r.costUsd)}</div>
        <div className="muted">
          {fmt.int(r.calls)} llamadas · cache hit {fmt.pct(r.cacheHit)}
        </div>
      </>
    ),
  }));

const useItems = (rows: BreakdownRow[]): BarItem[] =>
  rows.map((r) => ({
    key: r.key,
    label: r.label,
    value: r.calls,
    valueLabel: fmt.int(r.calls),
    badge: r.errors ? `${fmt.pct(r.errors / r.calls)} error` : undefined,
    tooltip: (
      <>
        <b>{r.label}</b>
        <div>{fmt.int(r.calls)} usos</div>
        <div className="muted">
          {fmt.int(r.errors)} con error ({fmt.pct(r.calls ? r.errors / r.calls : 0)})
        </div>
      </>
    ),
  }));

export function Dashboard({
  filter,
  singleProject,
  budget,
  refresh,
}: {
  filter: Filter;
  /** Nombre del proyecto si hay exactamente uno seleccionado. */
  singleProject: string | null;
  budget: number | null;
  refresh: number;
}) {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    // Gasto del mes: respeta agentes y proyectos, ignora el periodo.
    const monthFilter: Filter = { ...filter, from: monthStart().getTime(), to: undefined };
    Promise.all([
      api.summary(filter),
      api.timeseries(monthFilter, "day"),
      api.breakdown(filter, "project"),
      singleProject ? api.breakdown(filter, "branch") : Promise.resolve(null),
      api.breakdown(filter, "model"),
      api.breakdown(filter, "activity"),
      api.breakdown(filter, "tool"),
      api.breakdown(filter, "command"),
    ])
      .then(([summary, month, projects, branches, models, activity, tools, commands]) => {
        if (alive) {
          setData({ summary, month, projects, branches, models, activity, tools, commands });
          setError(null);
        }
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [filter, singleProject, refresh]);

  if (error) return <div className="main error">No se pudieron cargar los datos: {error}</div>;
  if (!data) return <div className="main muted">Cargando…</div>;
  const { summary } = data;

  return (
    <div className="main">
      {summary.calls === 0 && (
        <div className="notice">
          No hay llamadas en este periodo. Si acabas de instalar la app, espera a que termine el escaneo inicial o elige "Todo".
        </div>
      )}
      <Kpis s={summary} />
      {summary.unpricedModels.length > 0 && (
        <p className="muted small">
          Modelos sin precio (cuentan con coste 0): {summary.unpricedModels.join(", ")}
        </p>
      )}
      <div className="grid">
        <Panel title="Gasto del mes" question="¿Me paso este mes?" className="span-2">
          <MonthSpend points={data.month} budget={budget} />
        </Panel>
        {data.branches ? (
          <Panel title={`Ramas de ${singleProject}`} question="¿Cuánto costó cada feature?">
            <BarList items={costItems(data.branches)} />
          </Panel>
        ) : (
          <Panel title="Por proyecto" question="¿Cuánto costó cada proyecto?">
            <BarList items={costItems(data.projects)} />
          </Panel>
        )}
        <Panel title="Actividad" question="¿En qué se va el gasto?">
          <BarList items={costItems(data.activity, (r) => ACTIVITY_LABELS[r.key] ?? r.label)} />
        </Panel>
        <Panel title="Modelos" question="¿Uso el modelo adecuado?" className="span-2">
          <ModelsTable rows={data.models} />
        </Panel>
        <Panel title="Herramientas" question="¿Qué usa más y dónde falla?">
          <BarList items={useItems(data.tools)} />
        </Panel>
        <Panel title="Comandos de shell" question="¿Qué comandos ejecuta?">
          <BarList items={useItems(data.commands)} />
        </Panel>
      </div>
    </div>
  );
}
