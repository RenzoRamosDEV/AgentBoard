/**
 * Cada apartado como par tabla + gráfico, reutilizado por la vista general (recortado)
 * y por la vista ampliada (completo).
 */
import { useState, type ReactNode } from "react";
import type { ActivityRow, BreakdownRow, Point } from "../lib/api";
import { activityColor, activityLabel, agentColor, fmt, modelName } from "../lib/format";
import type { DashboardData } from "../lib/useData";
import { Bars, Columns, Donut, InlineBar, Legend, LineChart, Segmented } from "../components/Charts";
import { DataTable, type Column } from "../components/DataTable";
import { ChartTitle, Split } from "../components/Panel";

/** Columna con la barra de reparto integrada en la fila (no repite etiqueta ni cifra). */
function barColumn<T>(header: string, rows: T[], value: (r: T) => number, color: string): Column<T> {
  const max = Math.max(...rows.map(value), 0);
  return { header, cell: (r) => <InlineBar value={value(r)} max={max} color={color} />, width: "minmax(90px, 1.3fr)" };
}

const cost = (n: number) => fmt.usd(n);
const shot = (v: number | null | undefined) => (v == null ? "–" : fmt.pct(v));
const shotClass = (v: number | null | undefined) => (v == null ? "muted" : v >= 0.95 ? "good" : "warn");
const errClass = (r: BreakdownRow) => (r.calls && r.errors / r.calls > 0.05 ? "warn" : "muted");
const err = (r: BreakdownRow) => (r.errors ? fmt.pct(r.errors / r.calls) : "–");

export interface PanelProps {
  data: DashboardData;
  /** `true` en la vista ampliada: sin recorte y con gráficos grandes. */
  full: boolean;
  singleProject: string | null;
  /** Abre la vista ampliada (vista general). */
  open?: () => void;
}

/** Filas visibles en la vista general: todos los paneles iguales. */
const PREVIEW = 5;

// --- Daily Activity -------------------------------------------------------------

const DAY = 864e5;
const startOfDay = (ts: number) => {
  const d = new Date(ts);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
};

/** Un punto por día del periodo, con los días sin actividad a cero, para que la línea temporal sea continua. */
function dayPoints(daily: Point[], filter: { from?: number; to?: number }) {
  const byDay = new Map(daily.map((p) => [startOfDay(p.ts), p]));
  const today = startOfDay(Date.now());
  const first = daily.length ? startOfDay(daily[0].ts) : today;
  let start = filter.from != null ? startOfDay(filter.from) : first;
  let end = filter.to != null ? startOfDay(filter.to - 1) : today;
  if (end - start > 400 * DAY) start = end - 400 * DAY; // "Todo" con años de historial: agrupar es cosa de la vista ampliada
  const out = [];
  for (let d = new Date(start); d.getTime() <= end; d.setDate(d.getDate() + 1)) {
    const ts = d.getTime();
    const p = byDay.get(ts) ?? { ts, costUsd: 0, calls: 0 };
    out.push({
      ts,
      value: p.costUsd,
      tooltip: (
        <>
          <b>{fmt.date(ts)}</b>
          <div>{cost(p.costUsd)}</div>
          <div className="muted">{fmt.int(p.calls)} llamadas</div>
        </>
      ),
    });
  }
  return out;
}

type DailyMetric = "cost" | "calls" | "tokens";
type DailySplit = "total" | "agent" | "activity";

const METRICS: { value: DailyMetric; label: string }[] = [
  { value: "cost", label: "Coste" },
  { value: "calls", label: "Llamadas" },
  { value: "tokens", label: "Tokens de salida" },
];
const SPLITS: { value: DailySplit; label: string }[] = [
  { value: "total", label: "Total" },
  { value: "agent", label: "Por agente" },
  { value: "activity", label: "Por actividad" },
];

const metricOf = (m: DailyMetric) => ({
  value: (p: { costUsd: number; calls: number; outputTokens: number }) => (m === "cost" ? p.costUsd : m === "calls" ? p.calls : p.outputTokens),
  format: (v: number) => (m === "cost" ? cost(v) : m === "tokens" ? fmt.compact(v) : fmt.int(v)),
  title: m === "cost" ? "Coste por día (USD)" : m === "calls" ? "Llamadas por día" : "Tokens de salida por día",
});

/** Vista ampliada de Daily Activity: tabla, gráfico configurable, acumulado y reparto por hora. */
export function DailyFull({ data }: { data: DashboardData }) {
  const [metric, setMetric] = useState<DailyMetric>("cost");
  const [split, setSplit] = useState<DailySplit>("total");
  const m = metricOf(metric);
  const days = dayPoints(data.daily, data.filter);
  const byDay = new Map(data.daily.map((p) => [startOfDay(p.ts), p]));

  // Series apiladas por agente o actividad.
  const seriesByDay = new Map<number, { key: string; label: string; value: number; color: string }[]>();
  const legend = new Map<string, { label: string; color: string }>();
  if (split === "agent") {
    const order = data.agents.map((a) => a.key);
    for (const s of data.dailyByAgent) {
      const color = agentColor(s.key, Math.max(order.indexOf(s.key), 0));
      legend.set(s.key, { label: s.label, color });
      const list = seriesByDay.get(startOfDay(s.ts)) ?? [];
      list.push({ key: s.key, label: s.label, value: m.value(s), color });
      seriesByDay.set(startOfDay(s.ts), list);
    }
  } else if (split === "activity") {
    for (const d of data.activityDaily) {
      const color = activityColor(d.activity);
      legend.set(d.activity, { label: activityLabel(d.activity), color });
      const list = seriesByDay.get(startOfDay(d.ts)) ?? [];
      // La actividad se calcula por turno: coste o turnos (no hay tokens por actividad).
      list.push({ key: d.activity, label: activityLabel(d.activity), value: metric === "cost" ? d.costUsd : d.turns, color });
      seriesByDay.set(startOfDay(d.ts), list);
    }
  }
  const points = days.map((d) => {
    const p = byDay.get(d.ts);
    const stack = seriesByDay.get(d.ts)?.sort((a, b) => b.value - a.value);
    const value = split === "total" ? (p ? m.value(p) : 0) : (stack ?? []).reduce((a, s) => a + s.value, 0);
    return {
      ts: d.ts,
      value,
      stack: split === "total" ? undefined : stack ?? [],
      tooltip: (
        <>
          <b>{fmt.date(d.ts)}</b>
          <div>{m.format(value)}</div>
          {p && split === "total" && (
            <div className="muted">
              {fmt.int(p.calls)} llamadas · {fmt.int(p.sessions)} sesiones
            </div>
          )}
          {stack?.slice(0, 5).map((s) => (
            <div key={s.key} className="muted">
              {s.label}: {m.format(s.value)}
            </div>
          ))}
        </>
      ),
    };
  });
  const splitNote = split === "activity" && metric !== "cost" ? "Por actividad se cuentan turnos, no llamadas ni tokens." : null;

  // Acumulado del periodo.
  let acc = 0;
  const cumulative = days.map((d) => {
    const p = byDay.get(d.ts);
    acc += p ? m.value(p) : 0;
    return {
      ts: d.ts,
      value: acc,
      tooltip: (
        <>
          <b>{fmt.date(d.ts)}</b>
          <div>Acumulado: {m.format(acc)}</div>
          <div className="muted">Ese día: {m.format(p ? m.value(p) : 0)}</div>
        </>
      ),
    };
  });

  // Reparto por hora del día (hora local).
  const hours = Array.from({ length: 24 }, (_, h) => ({ h, value: 0, calls: 0 }));
  for (const p of data.hourly) {
    const h = new Date(p.ts).getHours();
    hours[h].value += m.value(p);
    hours[h].calls += p.calls;
  }
  const hourPoints = hours.map((x) => ({
    ts: x.h,
    value: x.value,
    tooltip: (
      <>
        <b>{String(x.h).padStart(2, "0")}:00 – {String((x.h + 1) % 24).padStart(2, "0")}:00</b>
        <div>{m.format(x.value)}</div>
        <div className="muted">{fmt.int(x.calls)} llamadas</div>
      </>
    ),
  }));
  const busiest = hours.reduce((a, b) => (b.value > a.value ? b : a), hours[0]);

  const columns: Column<Point>[] = [
    { header: "Día", cell: (p) => fmt.date(p.ts), width: "120px", className: "secondary" },
    { header: "Coste", cell: (p) => cost(p.costUsd), align: "right", className: "cost" },
    { header: "Llamadas", cell: (p) => fmt.int(p.calls), align: "right" },
    { header: "Sesiones", cell: (p) => fmt.int(p.sessions), align: "right", className: "secondary" },
    { header: "Entrada", cell: (p) => fmt.compact(p.inputTokens + p.cacheRead + p.cacheWrite), align: "right", className: "secondary" },
    { header: "Salida", cell: (p) => fmt.compact(p.outputTokens), align: "right", className: "secondary" },
    { header: "Cache hit", cell: (p) => { const t = p.inputTokens + p.cacheRead + p.cacheWrite; return t ? fmt.pct(p.cacheRead / t) : "–"; }, align: "right", width: "72px", className: "secondary" },
    barColumn("Reparto del coste", data.daily, (p) => p.costUsd, "var(--accent)"),
  ];
  const rows = [...data.daily].reverse();

  return (
    <>
      <section className="panel">
        <header className="panel-head">
          <div className="panel-title">
            <h2>Tabla por día</h2>
            <span className="muted">{rows.length} días con actividad, el más reciente primero</span>
          </div>
        </header>
        <DataTable rows={rows} rowKey={(p) => String(p.ts)} columns={columns} />
      </section>
      <section className="panel">
        <header className="panel-head">
          <div className="panel-title">
            <h2>Gráfico</h2>
            <span className="muted">{m.title}</span>
          </div>
          <div className="controls">
            <Segmented value={metric} options={METRICS} onChange={setMetric} label="Métrica" />
            <Segmented value={split} options={SPLITS} onChange={setSplit} label="Desglose" />
          </div>
        </header>
        <Columns points={points} format={m.format} height={260} color="var(--accent)" />
        {legend.size > 0 && <Legend items={[...legend.values()]} />}
        {splitNote && <p className="muted small">{splitNote}</p>}
      </section>
      <div className="grid-2">
        <section className="panel">
          <header className="panel-head">
            <div className="panel-title">
              <h2>Acumulado del periodo</h2>
              <span className="muted">{m.format(acc)} en total</span>
            </div>
          </header>
          <LineChart points={cumulative} format={m.format} height={200} />
        </section>
        <section className="panel">
          <header className="panel-head">
            <div className="panel-title">
              <h2>Por hora del día</h2>
              <span className="muted">hora local · más actividad a las {String(busiest.h).padStart(2, "0")}:00</span>
            </div>
          </header>
          <Columns points={hourPoints} format={m.format} height={200} color="var(--series-blue)" axis={(h) => `${String(h).padStart(2, "0")}h`} />
        </section>
      </div>
    </>
  );
}

export function DailyPanel({ data, full }: PanelProps) {
  const rows = [...data.daily].reverse();
  const columns: Column<Point>[] = [
    { header: "Día", cell: (p) => fmt.day(p.ts), width: "80px", className: "secondary" },
    { header: "Coste", cell: (p) => cost(p.costUsd), align: "right", className: "cost" },
    { header: "Llamadas", cell: (p) => fmt.int(p.calls), align: "right" },
  ];
  const table = <DataTable rows={rows} rowKey={(p) => String(p.ts)} columns={columns} limit={full ? undefined : PREVIEW} />;
  const chart = (
    <>
      <ChartTitle>Coste por día (USD)</ChartTitle>
      <Columns points={dayPoints(data.daily, data.filter)} format={cost} height={full ? 260 : 170} />
    </>
  );
  return full ? <Stacked table={table} chart={chart} /> : <Split table={table} chart={chart} chartWidth={400} />;
}

// --- By Agent ---------------------------------------------------------------------------

export function AgentPanel({ data, full }: PanelProps) {
  const rows = data.agents;
  const total = rows.reduce((a, r) => a + r.costUsd, 0);
  const color = (r: BreakdownRow) => agentColor(r.key, rows.indexOf(r));
  const columns: Column<BreakdownRow>[] = [
    {
      header: "Agente",
      cell: (r) => (
        <span className="with-dot">
          <i style={{ background: color(r) }} />
          {r.label}
        </span>
      ),
    },
    { header: "Coste", cell: (r) => cost(r.costUsd), align: "right", width: "64px", className: "cost" },
    { header: "Llamadas", cell: (r) => fmt.int(r.calls), align: "right", width: "60px" },
    ...(full
      ? [
          { header: "Sesiones", cell: (r: BreakdownRow) => fmt.int(r.sessions), align: "right" as const, width: "64px", className: "secondary" },
          { header: "Caché", cell: (r: BreakdownRow) => (r.cacheHit ? fmt.pct(r.cacheHit) : "–"), align: "right" as const, width: "56px", className: "secondary" },
          barColumn("Reparto del coste", rows, (r) => r.costUsd, "var(--series-blue)"),
        ]
      : []),
  ];
  const segments = rows.map((r) => ({ key: r.key, label: r.label, value: r.costUsd, color: color(r) }));
  const donut = (
    <>
      <ChartTitle>Reparto del coste</ChartTitle>
      <Donut segments={segments} center={cost(total)} format={cost} size={full ? 180 : 130} />
    </>
  );
  if (!rows.length) return <p className="empty">Todavía no se ha detectado ningún agente</p>;
  const table = <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} />;
  if (full) {
    return (
      <>
        {table}
        <div className="chart-box">{donut}</div>
      </>
    );
  }
  return (
    <div className="agent-panel">
      {donut}
      {table}
    </div>
  );
}

// --- By Project / By Branch -------------------------------------------------------

export function ProjectPanel({ data, full, singleProject }: PanelProps) {
  const rows = data.branches ?? data.projects;
  // En la vista general caben nombre, coste y sesiones; la ampliada añade media y overhead.
  const columns: Column<BreakdownRow>[] = [
    { header: singleProject ? "Rama" : "Proyecto", cell: (r) => r.label },
    { header: "Coste", cell: (r) => cost(r.costUsd), align: "right", width: "68px", className: "cost" },
    ...(full ? [{ header: "avg/ses", cell: (r: BreakdownRow) => cost(r.sessions ? r.costUsd / r.sessions : 0), align: "right" as const, width: "68px" }] : []),
    { header: "Ses", cell: (r) => fmt.int(r.sessions), align: "right", width: "40px", className: "secondary" },
    ...(full ? [{ header: "Overhead", cell: (r: BreakdownRow) => (r.overheadTokens ? fmt.compact(r.overheadTokens) : "–"), align: "right" as const, width: "70px", className: "accent" }] : []),
    barColumn("Reparto del coste", rows, (r) => r.costUsd, "var(--series-blue)"),
  ];
  return <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} limit={full ? undefined : PREVIEW} />;
}

// --- By Activity ------------------------------------------------------------------

export function ActivityPanel({ data, full }: PanelProps) {
  const rows = data.activity.activities;
  const total = rows.reduce((a, r) => a + r.costUsd, 0);
  const columns: Column<ActivityRow>[] = [
    {
      header: "Actividad",
      cell: (r) => (
        <span className="with-dot">
          <i style={{ background: activityColor(r.key) }} />
          {activityLabel(r.key)}
        </span>
      ),
    },
    { header: "Coste", cell: (r) => cost(r.costUsd), align: "right", width: "64px", className: "cost" },
    { header: "Turnos", cell: (r) => fmt.int(r.turns), align: "right", width: "52px" },
    ...(full
      ? [
          { header: "$/turno", cell: (r: ActivityRow) => cost(r.turns ? r.costUsd / r.turns : 0), align: "right" as const, width: "72px", className: "secondary" },
          { header: "Con edición", cell: (r: ActivityRow) => (r.editTurns ? fmt.int(r.editTurns) : "–"), align: "right" as const, width: "84px", className: "secondary" },
        ]
      : []),
    { header: "1-shot", cell: (r) => shot(r.oneShot), align: "right", width: "56px", className: (r) => shotClass(r.oneShot) },
  ];
  const table = <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} limit={full ? undefined : PREVIEW} />;
  const segments = rows.map((r) => ({ key: r.key, label: activityLabel(r.key), value: r.costUsd, color: activityColor(r.key) }));
  if (!full) {
    return (
      <Split
        table={table}
        chart={
          <>
            <ChartTitle>Reparto del coste</ChartTitle>
            <Donut segments={segments} center={cost(total)} format={cost} size={130} />
          </>
        }
        chartWidth={150}
      />
    );
  }
  // Vista ampliada: apilado por día + 1-shot por actividad.
  const byDay = new Map<number, { key: string; value: number; color: string; label: string }[]>();
  for (const d of data.activityDaily) {
    const list = byDay.get(d.ts) ?? [];
    list.push({ key: d.activity, value: d.costUsd, color: activityColor(d.activity), label: activityLabel(d.activity) });
    byDay.set(d.ts, list);
  }
  const stackPoints = [...byDay.entries()]
    .sort((a, b) => a[0] - b[0])
    .map(([ts, stack]) => {
      const sorted = [...stack].sort((a, b) => b.value - a.value);
      const value = stack.reduce((a, s) => a + s.value, 0);
      return {
        ts,
        value,
        stack: sorted,
        tooltip: (
          <>
            <b>{fmt.date(ts)}</b>
            <div>{cost(value)}</div>
            {sorted.slice(0, 4).map((s) => (
              <div key={s.key} className="muted">
                {s.label}: {cost(s.value)}
              </div>
            ))}
          </>
        ),
      };
    });
  const shots = rows.filter((r) => r.oneShot != null).map((r) => ({ key: r.key, label: activityLabel(r.key), value: r.oneShot!, valueLabel: fmt.pct(r.oneShot!), color: activityColor(r.key) }));
  return (
    <>
      {table}
      <div className="grid-2">
        <div className="chart-box">
          <ChartTitle>Coste por día y actividad</ChartTitle>
          <Columns points={stackPoints} format={cost} height={220} />
          <Legend items={rows.slice(0, 6).map((r) => ({ label: activityLabel(r.key), color: activityColor(r.key) }))} />
        </div>
        <div className="chart-box">
          <ChartTitle>1-shot por actividad (solo con ediciones)</ChartTitle>
          {shots.length ? <Bars items={shots} thick labelWidth={120} /> : <p className="empty">Ningún turno con ediciones en este periodo</p>}
          <p className="muted small">Un turno es 1-shot cuando ninguna edición falla y ningún archivo se edita dos veces.</p>
        </div>
      </div>
    </>
  );
}

// --- By Model ---------------------------------------------------------------------

export function ModelPanel({ data, full }: PanelProps) {
  const rows = data.models;
  const oneShot = new Map(data.activity.models.map((m) => [m.model, m.oneShot]));
  const columns: Column<BreakdownRow>[] = [
    {
      header: "Modelo",
      cell: (r) => (
        <>
          {modelName(r.key)}
          {!r.hasPrice && <span className="badge">sin precio</span>}
        </>
      ),
    },
    { header: "Coste", cell: (r) => cost(r.costUsd), align: "right", width: "68px", className: "cost" },
    ...(full ? [{ header: "Caché", cell: (r: BreakdownRow) => (r.cacheHit ? fmt.pct(r.cacheHit) : "–"), align: "right" as const, width: "56px", className: "secondary" }] : []),
    { header: "Llamadas", cell: (r) => fmt.int(r.calls), align: "right", width: "60px" },
    { header: "1-shot", cell: (r) => shot(oneShot.get(r.key)), align: "right", width: "56px", className: (r) => shotClass(oneShot.get(r.key)) },
    barColumn("Reparto del coste", rows, (r) => r.costUsd, "var(--series-violet)"),
  ];
  return <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} limit={full ? undefined : PREVIEW} />;
}

// --- Tools / Shell / MCP (usos y errores) ----------------------------------------------

function UsesPanel({ rows, full, header, color, mono = false }: { rows: BreakdownRow[]; full: boolean; header: string; color: string; mono?: boolean }) {
  const columns: Column<BreakdownRow>[] = [
    { header, cell: (r) => r.label, className: mono ? "mono" : "" },
    { header: "Llamadas", cell: (r) => fmt.int(r.calls), align: "right", width: "64px" },
    { header: "Errores", cell: err, align: "right", width: "60px", className: errClass },
    barColumn("Uso", rows, (r) => r.calls, color),
  ];
  return <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} limit={full ? undefined : PREVIEW} />;
}

export const ToolsPanel = ({ data, full }: PanelProps) => <UsesPanel rows={data.tools} full={full} header="Herramienta" color="var(--series-green)" />;
export const ShellPanel = ({ data, full }: PanelProps) => <UsesPanel rows={data.commands} full={full} header="Comando" color="var(--series-yellow)" mono />;
export const McpPanel = ({ data, full }: PanelProps) => <UsesPanel rows={data.mcp} full={full} header="Servidor" color="var(--series-magenta)" />;

// --- Skills & Agents / Claude Agent Types (usos y coste) ---------------------------------

function CostUsesPanel({ rows, full, header, usesHeader, color }: { rows: BreakdownRow[]; full: boolean; header: string; usesHeader: string; color: string }) {
  const columns: Column<BreakdownRow>[] = [
    { header, cell: (r) => r.label },
    { header: usesHeader, cell: (r) => fmt.int(r.calls), align: "right", width: "64px" },
    { header: "Coste", cell: (r) => cost(r.costUsd), align: "right", className: "cost" },
    barColumn("Reparto del coste", rows, (r) => r.costUsd, color),
  ];
  return <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} limit={full ? undefined : PREVIEW} />;
}

export const SkillsPanel = ({ data, full }: PanelProps) => <CostUsesPanel rows={data.skills} full={full} header="Skill / agente" usesHeader="Usos" color="var(--series-violet)" />;
export const AgentTypesPanel = ({ data, full }: PanelProps) => <CostUsesPanel rows={data.agentTypes} full={full} header="Tipo" usesHeader="Llamadas" color="var(--series-blue)" />;

// --- Auxiliares --------------------------------------------------------------------

/** Vista ampliada: tabla completa arriba y gráfico grande debajo. */
const Stacked = ({ table, chart }: { table: ReactNode; chart: ReactNode }) => (
  <>
    {table}
    <div className="chart-box">{chart}</div>
  </>
);
