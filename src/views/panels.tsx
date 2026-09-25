/**
 * Cada apartado como par tabla + gráfico, reutilizado por la vista general (recortado)
 * y por la vista ampliada (completo).
 */
import type { ReactNode } from "react";
import type { ActivityRow, BreakdownRow, Point } from "../lib/api";
import { activityColor, activityLabel, fmt, modelName } from "../lib/format";
import type { DashboardData } from "../lib/useData";
import { Bars, Columns, Donut, InlineBar, Legend } from "../components/Charts";
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
  return full ? <Stacked table={table} chart={chart} /> : <Split table={table} chart={chart} chartWidth={480} />;
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
