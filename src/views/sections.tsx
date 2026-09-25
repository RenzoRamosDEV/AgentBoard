/**
 * Vistas ampliadas de cada apartado: tarjeta de tabla arriba y, debajo, los gráficos que
 * tienen sentido para ese apartado (reparto, ranking con métrica, evolución por día…).
 */
import { useState, type ReactNode } from "react";
import type { ActivityRow, BreakdownRow, SeriesPoint } from "../lib/api";
import { activityColor, activityLabel, agentColor, fmt, modelName, paletteColor } from "../lib/format";
import type { DashboardData } from "../lib/useData";
import { Bars, Columns, Legend, Segmented, ShareBar } from "../components/Charts";
import { DataTable, type Column } from "../components/DataTable";
import { barColumn, cost, dayPoints, err, errClass, shot, shotClass, startOfDay } from "./panels";

// --- Bloques genéricos ------------------------------------------------------------------

export const Card = ({ title, subtitle, actions, children }: { title: string; subtitle?: ReactNode; actions?: ReactNode; children: ReactNode }) => (
  <section className="panel">
    <header className="panel-head">
      <div className="panel-title">
        <h2>{title}</h2>
        {subtitle && <span className="muted">{subtitle}</span>}
      </div>
      {actions && <div className="controls">{actions}</div>}
    </header>
    {children}
  </section>
);

interface Metric<T> {
  value: string;
  label: string;
  get: (r: T) => number;
  format: (v: number) => string;
}

const M = {
  cost: { value: "cost", label: "Coste", get: (r: { costUsd: number }) => r.costUsd, format: cost },
  calls: { value: "calls", label: "Llamadas", get: (r: { calls: number }) => r.calls, format: fmt.int },
  uses: { value: "calls", label: "Usos", get: (r: { calls: number }) => r.calls, format: fmt.int },
  sessions: { value: "sessions", label: "Sesiones", get: (r: { sessions: number }) => r.sessions, format: fmt.int },
  errorRate: { value: "errors", label: "% error", get: (r: { calls: number; errors: number }) => (r.calls ? r.errors / r.calls : 0), format: fmt.pct },
  cacheHit: { value: "cache", label: "Cache hit", get: (r: { cacheHit: number }) => r.cacheHit, format: fmt.pct },
  avgSession: { value: "avg", label: "$/sesión", get: (r: { costUsd: number; sessions: number }) => (r.sessions ? r.costUsd / r.sessions : 0), format: cost },
  tokens: { value: "tokens", label: "Tokens de salida", get: (r: { outputTokens: number }) => r.outputTokens, format: fmt.compact },
};

/** Barras horizontales con selector de métrica: como mucho `limit` filas; la última agrupa el resto. */
function RankingCard<T extends { key: string }>({
  title,
  rows,
  label,
  color,
  metrics,
  limit = 7,
  othersLabel = "Otros",
}: {
  title: string;
  rows: T[];
  label: (r: T) => string;
  color: (r: T, i: number) => string;
  metrics: Metric<T>[];
  limit?: number;
  othersLabel?: string;
}) {
  const [metric, setMetric] = useState(metrics[0].value);
  const m = metrics.find((x) => x.value === metric) ?? metrics[0];
  const isRate = m.value === "errors" || m.value === "cache" || m.value === "avg";
  const sorted = rows.map((r, i) => ({ r, i })).sort((a, b) => m.get(b.r) - m.get(a.r));
  const head = sorted.length > limit ? sorted.slice(0, limit - 1) : sorted;
  const tail = sorted.slice(head.length);
  const items = head.map(({ r, i }) => ({
    key: r.key,
    label: label(r),
    value: m.get(r),
    valueLabel: m.format(m.get(r)),
    color: color(r, i),
    tooltip: (
      <>
        <b>{label(r)}</b>
        <div>{m.format(m.get(r))}</div>
      </>
    ),
  }));
  if (tail.length) {
    // Los porcentajes y medias no se suman: para "otros" se usa la media.
    const values = tail.map(({ r }) => m.get(r));
    const value = isRate ? values.reduce((a, v) => a + v, 0) / values.length : values.reduce((a, v) => a + v, 0);
    items.push({
      key: "__otros",
      label: `${othersLabel} (${tail.length})`,
      value,
      valueLabel: m.format(value),
      color: "var(--text-muted)",
      tooltip: (
        <>
          <b>{othersLabel}</b>
          <div>
            {tail.length} elementos · {isRate ? "media" : "suma"}: {m.format(value)}
          </div>
          {tail.slice(0, 6).map(({ r }) => (
            <div key={r.key} className="muted">
              {label(r)}: {m.format(m.get(r))}
            </div>
          ))}
        </>
      ),
    });
  }
  return (
    <Card title={title} subtitle={m.label} actions={metrics.length > 1 && <Segmented value={metric} options={metrics.map((x) => ({ value: x.value, label: x.label }))} onChange={setMetric} label="Métrica" />}>
      <Bars items={items} thick labelWidth={150} />
    </Card>
  );
}

/** Anillo de reparto de una métrica. */
function ShareCard<T extends { key: string }>({ title, rows, label, color, metric, othersLabel }: { title: string; rows: T[]; label: (r: T) => string; color: (r: T, i: number) => string; metric: Metric<T>; othersLabel?: string }) {
  const total = rows.reduce((a, r) => a + metric.get(r), 0);
  const segments = rows.map((r, i) => ({ key: r.key, label: label(r), value: metric.get(r), color: color(r, i) }));
  return (
    <Card title={title} subtitle={`${metric.label.toLowerCase()} · ${metric.format(total)} en total`}>
      <div className="share-card">
        <ShareBar segments={segments} format={metric.format} limit={7} othersLabel={othersLabel} />
      </div>
    </Card>
  );
}

type SeriesMetric = "cost" | "calls" | "tokens";
const SERIES_METRICS: { value: SeriesMetric; label: string }[] = [
  { value: "cost", label: "Coste" },
  { value: "calls", label: "Llamadas" },
  { value: "tokens", label: "Tokens de salida" },
];

/** Columnas apiladas por día para una serie (agente, modelo, proyecto…), con métrica a elegir. */
function EvolutionCard({
  title,
  series,
  data,
  color,
  label = (s) => s.label,
  metrics = SERIES_METRICS,
  maxKeys = 8,
}: {
  title: string;
  series: SeriesPoint[];
  data: DashboardData;
  color: (key: string, index: number) => string;
  label?: (s: SeriesPoint) => string;
  metrics?: { value: SeriesMetric; label: string }[];
  maxKeys?: number;
}) {
  const [metric, setMetric] = useState<SeriesMetric>(metrics[0].value);
  const get = (s: { costUsd: number; calls: number; outputTokens: number }) => (metric === "cost" ? s.costUsd : metric === "calls" ? s.calls : s.outputTokens);
  const format = metric === "cost" ? cost : metric === "tokens" ? fmt.compact : fmt.int;

  // Las claves con más peso tienen color propio; el resto se agrupa en "Otros".
  const totals = new Map<string, { label: string; value: number }>();
  for (const s of series) {
    const t = totals.get(s.key) ?? { label: label(s), value: 0 };
    t.value += get(s);
    totals.set(s.key, t);
  }
  const ranked = [...totals.entries()].sort((a, b) => b[1].value - a[1].value);
  const colors = new Map(ranked.slice(0, maxKeys).map(([key], i) => [key, color(key, i)]));
  const legend = ranked.slice(0, maxKeys).map(([key, t]) => ({ label: t.label, color: colors.get(key)! }));
  if (ranked.length > maxKeys) legend.push({ label: "Otros", color: "var(--text-muted)" });

  const byDay = new Map<number, Map<string, { key: string; label: string; value: number; color: string }>>();
  for (const s of series) {
    const day = startOfDay(s.ts);
    const key = colors.has(s.key) ? s.key : "__otros";
    const map = byDay.get(day) ?? new Map();
    const cur = map.get(key) ?? { key, label: key === "__otros" ? "Otros" : label(s), value: 0, color: colors.get(s.key) ?? "var(--text-muted)" };
    cur.value += get(s);
    map.set(key, cur);
    byDay.set(day, map);
  }
  const points = dayPoints(data.daily, data.filter).map((d) => {
    const stack = [...(byDay.get(d.ts)?.values() ?? [])].sort((a, b) => b.value - a.value);
    const value = stack.reduce((a, s) => a + s.value, 0);
    return {
      ts: d.ts,
      value,
      stack,
      tooltip: (
        <>
          <b>{fmt.date(d.ts)}</b>
          <div>{format(value)}</div>
          {stack.slice(0, 6).map((s) => (
            <div key={s.key} className="muted">
              {s.label}: {format(s.value)}
            </div>
          ))}
        </>
      ),
    };
  });
  return (
    <Card title={title} subtitle="por día, apilado" actions={metrics.length > 1 && <Segmented value={metric} options={metrics} onChange={setMetric} label="Métrica" />}>
      <Columns points={points} format={format} height={240} />
      <Legend items={legend} />
    </Card>
  );
}

const rowColor = (rows: { key: string }[]) => (r: { key: string }) => paletteColor(rows.findIndex((x) => x.key === r.key));

// --- Apartados ----------------------------------------------------------------------------

export function AgentFull({ data }: { data: DashboardData }) {
  const rows = data.agents;
  const color = (r: BreakdownRow, i: number) => agentColor(r.key, i);
  const columns: Column<BreakdownRow>[] = [
    { header: "Agente", cell: (r) => <span className="with-dot"><i style={{ background: color(r, rows.indexOf(r)) }} />{r.label}</span> },
    { header: "Coste", cell: (r) => cost(r.costUsd), align: "right", className: "cost" },
    { header: "Llamadas", cell: (r) => fmt.int(r.calls), align: "right" },
    { header: "Sesiones", cell: (r) => fmt.int(r.sessions), align: "right", className: "secondary" },
    { header: "$/sesión", cell: (r) => cost(r.sessions ? r.costUsd / r.sessions : 0), align: "right", width: "76px", className: "secondary" },
    { header: "Cache hit", cell: (r) => (r.cacheHit ? fmt.pct(r.cacheHit) : "–"), align: "right", width: "72px", className: "secondary" },
    barColumn("Reparto del coste", rows, (r) => r.costUsd, "var(--series-blue)"),
  ];
  return (
    <>
      <Card title="Tabla" subtitle={`${rows.length} agentes detectados`}>
        <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} empty="Todavía no se ha detectado ningún agente" />
      </Card>
      <div className="grid-2">
        <ShareCard title="Reparto del coste" rows={rows} label={(r) => r.label} color={color} metric={M.cost} />
        <RankingCard title="Ranking" rows={rows} label={(r) => r.label} color={color} metrics={[M.cost, M.calls, M.sessions, M.avgSession, M.cacheHit]} othersLabel="Otros agentes" />
      </div>
      <EvolutionCard title="Evolución por agente" series={data.dailyByAgent} data={data} color={(key, i) => agentColor(key, i)} />
    </>
  );
}

export function ProjectFull({ data, singleProject }: { data: DashboardData; singleProject: string | null }) {
  const rows = data.branches ?? data.projects;
  const color = rowColor(rows);
  const what = singleProject ? "rama" : "proyecto";
  const columns: Column<BreakdownRow>[] = [
    { header: singleProject ? "Rama" : "Proyecto", cell: (r) => <span className="with-dot"><i style={{ background: color(r) }} />{r.label}</span> },
    { header: "Coste", cell: (r) => cost(r.costUsd), align: "right", className: "cost" },
    { header: "$/sesión", cell: (r) => cost(r.sessions ? r.costUsd / r.sessions : 0), align: "right", width: "76px" },
    { header: "Sesiones", cell: (r) => fmt.int(r.sessions), align: "right", className: "secondary" },
    { header: "Llamadas", cell: (r) => fmt.int(r.calls), align: "right", className: "secondary" },
    { header: "Overhead", cell: (r) => (r.overheadTokens ? fmt.compact(r.overheadTokens) : "–"), align: "right", width: "76px", className: "accent" },
    barColumn("Reparto del coste", rows, (r) => r.costUsd, "var(--series-blue)"),
  ];
  return (
    <>
      <Card title="Tabla" subtitle={`${rows.length} ${what}s · overhead = tokens de contexto con los que arranca cada sesión`}>
        <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} />
      </Card>
      <div className="grid-2">
        <ShareCard title="Reparto del coste" rows={rows} label={(r) => r.label} color={color} metric={M.cost} />
        <RankingCard title="Ranking" rows={rows} label={(r) => r.label} color={color} metrics={[M.cost, M.avgSession, M.sessions, M.calls]} othersLabel={singleProject ? "Otras ramas" : "Otros proyectos"} />
      </div>
      <EvolutionCard title={`Evolución por ${what}`} series={data.dailyByProject} data={data} color={(key) => color({ key })} />
    </>
  );
}

export function ActivityFull({ data }: { data: DashboardData }) {
  const rows = data.activity.activities;
  const [metric, setMetric] = useState<"cost" | "turns">("cost");
  const get = (r: { costUsd: number; turns: number }) => (metric === "cost" ? r.costUsd : r.turns);
  const format = metric === "cost" ? cost : fmt.int;
  const columns: Column<ActivityRow>[] = [
    { header: "Actividad", cell: (r) => <span className="with-dot"><i style={{ background: activityColor(r.key) }} />{activityLabel(r.key)}</span> },
    { header: "Coste", cell: (r) => cost(r.costUsd), align: "right", className: "cost" },
    { header: "Turnos", cell: (r) => fmt.int(r.turns), align: "right", width: "60px" },
    { header: "$/turno", cell: (r) => cost(r.turns ? r.costUsd / r.turns : 0), align: "right", width: "72px", className: "secondary" },
    { header: "Con edición", cell: (r) => (r.editTurns ? fmt.int(r.editTurns) : "–"), align: "right", width: "84px", className: "secondary" },
    { header: "1-shot", cell: (r) => shot(r.oneShot), align: "right", width: "60px", className: (r) => shotClass(r.oneShot) },
    barColumn("Reparto del coste", rows, (r) => r.costUsd, "var(--act-coding)"),
  ];
  const byDay = new Map<number, { key: string; label: string; value: number; color: string }[]>();
  for (const d of data.activityDaily) {
    const day = startOfDay(d.ts);
    const list = byDay.get(day) ?? [];
    list.push({ key: d.activity, label: activityLabel(d.activity), value: get({ costUsd: d.costUsd, turns: d.turns }), color: activityColor(d.activity) });
    byDay.set(day, list);
  }
  const points = dayPoints(data.daily, data.filter).map((d) => {
    const stack = (byDay.get(d.ts) ?? []).sort((a, b) => b.value - a.value);
    const value = stack.reduce((a, s) => a + s.value, 0);
    return {
      ts: d.ts,
      value,
      stack,
      tooltip: (
        <>
          <b>{fmt.date(d.ts)}</b>
          <div>{format(value)}</div>
          {stack.slice(0, 5).map((s) => (
            <div key={s.key} className="muted">
              {s.label}: {format(s.value)}
            </div>
          ))}
        </>
      ),
    };
  });
  const shots = rows.filter((r) => r.oneShot != null).map((r) => ({ key: r.key, label: activityLabel(r.key), value: r.oneShot!, valueLabel: fmt.pct(r.oneShot!), color: activityColor(r.key) }));
  const segments = rows.map((r) => ({ key: r.key, label: activityLabel(r.key), value: r.costUsd, color: activityColor(r.key) }));
  const total = rows.reduce((a, r) => a + r.costUsd, 0);
  return (
    <>
      <Card title="Tabla" subtitle="un turno = un prompt tuyo y todo lo que hace el agente hasta el siguiente">
        <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} />
      </Card>
      <Card
        title="Evolución por actividad"
        subtitle="por día, apilado"
        actions={<Segmented value={metric} options={[{ value: "cost", label: "Coste" }, { value: "turns", label: "Turnos" }]} onChange={setMetric} label="Métrica" />}
      >
        <Columns points={points} format={format} height={240} />
        <Legend items={rows.slice(0, 8).map((r) => ({ label: activityLabel(r.key), color: activityColor(r.key) }))} />
      </Card>
      <div className="grid-2">
        <Card title="Reparto del coste" subtitle={`${cost(total)} en total`}>
          <div className="share-card">
            <ShareBar segments={segments} format={cost} />
          </div>
        </Card>
        <Card title="1-shot por actividad" subtitle="solo actividades con ediciones">
          {shots.length ? <Bars items={shots} thick labelWidth={140} /> : <p className="empty">Ningún turno con ediciones en este periodo</p>}
          <p className="muted small">Un turno es 1-shot cuando ninguna edición falla y ningún archivo se edita dos veces.</p>
        </Card>
      </div>
    </>
  );
}

export function ModelFull({ data }: { data: DashboardData }) {
  const rows = data.models;
  const color = rowColor(rows);
  const oneShot = new Map(data.activity.models.map((m) => [m.model, m.oneShot]));
  const columns: Column<BreakdownRow>[] = [
    { header: "Modelo", cell: (r) => <span className="with-dot"><i style={{ background: color(r) }} />{modelName(r.key)}{!r.hasPrice && <span className="badge">sin precio</span>}</span> },
    { header: "Coste", cell: (r) => cost(r.costUsd), align: "right", className: "cost" },
    { header: "Llamadas", cell: (r) => fmt.int(r.calls), align: "right" },
    { header: "Sesiones", cell: (r) => fmt.int(r.sessions), align: "right", className: "secondary" },
    { header: "Cache hit", cell: (r) => (r.cacheHit ? fmt.pct(r.cacheHit) : "–"), align: "right", width: "72px", className: "secondary" },
    { header: "1-shot", cell: (r) => shot(oneShot.get(r.key)), align: "right", width: "60px", className: (r) => shotClass(oneShot.get(r.key)) },
    barColumn("Reparto del coste", rows, (r) => r.costUsd, "var(--series-violet)"),
  ];
  return (
    <>
      <Card title="Tabla" subtitle={`${rows.length} modelos · 1-shot solo en modelos con turnos de edición`}>
        <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} />
      </Card>
      <div className="grid-2">
        <ShareCard title="Reparto del coste" rows={rows} label={(r) => modelName(r.key)} color={color} metric={M.cost} />
        <RankingCard title="Ranking" rows={rows} label={(r) => modelName(r.key)} color={color} metrics={[M.cost, M.calls, M.cacheHit, M.sessions]} othersLabel="Otros modelos" />
      </div>
      <EvolutionCard title="Evolución por modelo" series={data.dailyByModel} data={data} color={(key) => color({ key })} label={(s) => modelName(s.key)} />
    </>
  );
}

function UsesFull({ rows, header, color, series, data, seriesTitle, mono = false, hint }: { rows: BreakdownRow[]; header: string; color: string; series?: SeriesPoint[]; data: DashboardData; seriesTitle?: string; mono?: boolean; hint?: string }) {
  const rc = rowColor(rows);
  const columns: Column<BreakdownRow>[] = [
    { header, cell: (r) => r.label, className: mono ? "mono" : "" },
    { header: "Llamadas", cell: (r) => fmt.int(r.calls), align: "right" },
    { header: "Errores", cell: (r) => fmt.int(r.errors), align: "right", width: "64px", className: "secondary" },
    { header: "% error", cell: err, align: "right", width: "64px", className: errClass },
    barColumn("Uso", rows, (r) => r.calls, color),
  ];
  return (
    <>
      <Card title="Tabla" subtitle={`${rows.length} ${header.toLowerCase()}s${hint ? ` · ${hint}` : ""}`}>
        <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} />
      </Card>
      <div className="grid-2">
        <RankingCard title="Ranking" rows={rows} label={(r) => r.label} color={() => color} metrics={[M.uses, M.errorRate]} othersLabel={`Otr${header === "Herramienta" ? "as herramientas" : header === "Comando" ? "os comandos" : "os servidores"}`} />
        <ShareCard title="Reparto de las llamadas" rows={rows.slice(0, 8)} label={(r) => r.label} color={rc} metric={M.uses} />
      </div>
      {series && seriesTitle && <EvolutionCard title={seriesTitle} series={series} data={data} color={(key) => rc({ key })} metrics={[{ value: "calls", label: "Llamadas" }]} />}
    </>
  );
}

export const ToolsFull = ({ data }: { data: DashboardData }) => (
  <UsesFull rows={data.tools} header="Herramienta" color="var(--series-green)" series={data.dailyByTool} seriesTitle="Evolución por herramienta" data={data} hint="sin las herramientas MCP" />
);
export const ShellFull = ({ data }: { data: DashboardData }) => (
  <UsesFull rows={data.commands} header="Comando" color="var(--series-yellow)" data={data} mono hint="primera palabra de cada comando, separando tuberías y encadenamientos" />
);
export const McpFull = ({ data }: { data: DashboardData }) => <UsesFull rows={data.mcp} header="Servidor" color="var(--series-magenta)" data={data} />;

function CostUsesFull({ rows, header, usesHeader, color, hint, othersLabel = "Otros" }: { rows: BreakdownRow[]; header: string; usesHeader: string; color: string; hint?: string; othersLabel?: string }) {
  const rc = rowColor(rows);
  const usesMetric = { ...M.uses, label: usesHeader };
  const columns: Column<BreakdownRow>[] = [
    { header, cell: (r) => <span className="with-dot"><i style={{ background: rc(r) }} />{r.label}</span> },
    { header: usesHeader, cell: (r) => fmt.int(r.calls), align: "right" },
    { header: "Coste", cell: (r) => cost(r.costUsd), align: "right", className: "cost" },
    { header: "$/uso", cell: (r) => cost(r.calls ? r.costUsd / r.calls : 0), align: "right", width: "72px", className: "secondary" },
    barColumn("Reparto del coste", rows, (r) => r.costUsd, color),
  ];
  return (
    <>
      <Card title="Tabla" subtitle={hint}>
        <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} />
      </Card>
      <div className="grid-2">
        <ShareCard title="Reparto del coste" rows={rows} label={(r) => r.label} color={rc} metric={M.cost} />
        <RankingCard title="Ranking" rows={rows} label={(r) => r.label} color={rc} metrics={[M.cost, usesMetric]} othersLabel={othersLabel} />
      </div>
    </>
  );
}

export const SkillsFull = ({ data }: { data: DashboardData }) => (
  <CostUsesFull rows={data.skills} header="Skill / agente" usesHeader="Usos" color="var(--series-violet)" hint="coste de las respuestas del modelo que los invocaron" othersLabel="Otras skills y agentes" />
);
export const AgentTypesFull = ({ data }: { data: DashboardData }) => (
  <CostUsesFull rows={data.agentTypes} header="Tipo" usesHeader="Llamadas" color="var(--series-blue)" hint="llamadas hechas dentro de subagentes, por tipo" othersLabel="Otros tipos" />
);
