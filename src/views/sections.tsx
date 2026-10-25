/**
 * Vistas ampliadas de cada apartado: tarjeta de tabla arriba y, debajo, los gráficos que
 * tienen sentido para ese apartado (reparto, ranking con métrica, evolución por día…).
 */
import { useMemo, useState, type ReactNode } from "react";
import type { ActivityRow, BreakdownRow, SeriesPoint } from "../lib/api";
import { activityColor, activityLabel, agentColor, fmt, modelName, paletteColor } from "../lib/format";
import type { DashboardData } from "../lib/useData";
import { t } from "../lib/i18n";
import { Bars, Columns, Legend, Segmented, ShareBar } from "../components/Charts";
import { DataTable, type Column } from "../components/DataTable";
import { barColumn, bucketing, cost, dayPoints, err, errClass, shot, shotClass } from "./panels";
import { rankItems, rankKeys, stacksByBucket } from "./derive";

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
  othersLabel,
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
  // `label` y `color` dependen solo de las filas en todos los usos: la memo va por filas y métrica.
  const items = useMemo(() => {
    const { head, tail, others } = rankItems(rows, m.get, isRate, limit);
    const out = head.map(({ r, i }) => ({
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
    if (others != null) {
      const name = othersLabel ?? t("Otros");
      out.push({
        key: "__otros",
        label: `${name} (${tail.length})`,
        value: others,
        valueLabel: m.format(others),
        color: "var(--text-muted)",
        tooltip: (
          <>
            <b>{name}</b>
            <div>{t("{n} elementos · {mode}: {v}", { n: tail.length, mode: t(isRate ? "media" : "suma"), v: m.format(others) })}</div>
            {tail.slice(0, 6).map(({ r }) => (
              <div key={r.key} className="muted">
                {label(r)}: {m.format(m.get(r))}
              </div>
            ))}
          </>
        ),
      });
    }
    return out;
  }, [rows, m, isRate, limit, othersLabel]);
  return (
    <Card title={title} subtitle={t(m.label)} actions={metrics.length > 1 && <Segmented value={metric} options={metrics.map((x) => ({ value: x.value, label: t(x.label) }))} onChange={setMetric} label={t("Métrica")} />}>
      <Bars items={items} thick labelWidth={150} />
    </Card>
  );
}

/** Anillo de reparto de una métrica. */
function ShareCard<T extends { key: string }>({ title, rows, label, color, metric, othersLabel }: { title: string; rows: T[]; label: (r: T) => string; color: (r: T, i: number) => string; metric: Metric<T>; othersLabel?: string }) {
  const total = rows.reduce((a, r) => a + metric.get(r), 0);
  const segments = rows.map((r, i) => ({ key: r.key, label: label(r), value: metric.get(r), color: color(r, i) }));
  return (
    <Card title={title} subtitle={`${t(metric.label).toLowerCase()} · ${t("{v} en total", { v: metric.format(total) })}`}>
      <div className="share-card">
        <ShareBar segments={segments} format={metric.format} limit={7} othersLabel={othersLabel ?? t("Otros")} />
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

  // `color` y `label` dependen solo de la clave en todos los usos: la memo va por serie, datos y métrica.
  const { legend, points } = useMemo(() => {
    // Las claves con más peso tienen color propio; el resto se agrupa en "Otros".
    const ranked = rankKeys(series, get, label);
    const colors = new Map(ranked.slice(0, maxKeys).map(({ key }, i) => [key, color(key, i)]));
    const legend = ranked.slice(0, maxKeys).map((x) => ({ label: x.label, color: colors.get(x.key)! }));
    if (ranked.length > maxKeys) legend.push({ label: t("Otros"), color: "var(--text-muted)" });

    const b = bucketing(data.daily, data.filter);
    const byDay = stacksByBucket(
      series,
      (s) => b.key(s.ts),
      (s) => (colors.has(s.key) ? s.key : "__otros"),
      (s) => (colors.has(s.key) ? label(s) : t("Otros")),
      get,
      (s) => colors.get(s.key) ?? "var(--text-muted)",
    );
    const points = dayPoints(data.daily, data.filter).map((d) => {
      const stack = [...(byDay.get(d.ts) ?? [])].sort((a, b) => b.value - a.value);
      const value = stack.reduce((a, s) => a + s.value, 0);
      return {
        ts: d.ts,
        value,
        stack,
        tooltip: (
          <>
            <b>{b.label(d.ts)}</b>
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
    return { legend, points };
  }, [series, data, metric, maxKeys]);
  return (
    <Card title={title} subtitle={t("por día, apilado")} actions={metrics.length > 1 && <Segmented value={metric} options={metrics.map((o) => ({ ...o, label: t(o.label) }))} onChange={setMetric} label={t("Métrica")} />}>
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
  const columns = useMemo((): Column<BreakdownRow>[] => [
    { header: t("Agente"), cell: (r) => <span className="with-dot"><i style={{ background: color(r, rows.indexOf(r)) }} />{r.label}</span> },
    { header: t("Coste"), cell: (r) => cost(r.costUsd), align: "right", className: "cost" },
    { header: t("Llamadas"), cell: (r) => fmt.int(r.calls), align: "right" },
    { header: t("Sesiones"), cell: (r) => fmt.int(r.sessions), align: "right", className: "secondary" },
    { header: t("$/sesión"), cell: (r) => cost(r.sessions ? r.costUsd / r.sessions : 0), align: "right", width: "76px", className: "secondary" },
    { header: t("Cache hit"), cell: (r) => (r.cacheHit ? fmt.pct(r.cacheHit) : "–"), align: "right", width: "72px", className: "secondary" },
    barColumn(t("Reparto del coste"), rows, (r) => r.costUsd, "var(--series-blue)"),
  ], [rows]);
  return (
    <>
      <Card title={t("Tabla")} subtitle={t("{n} agentes detectados", { n: rows.length })}>
        <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} empty={t("Todavía no se ha detectado ningún agente")} />
      </Card>
      <div className="grid-2">
        <ShareCard title={t("Reparto del coste")} rows={rows} label={(r) => r.label} color={color} metric={M.cost} />
        <RankingCard title={t("Ranking")} rows={rows} label={(r) => r.label} color={color} metrics={[M.cost, M.calls, M.sessions, M.avgSession, M.cacheHit]} othersLabel={t("Otros agentes")} />
      </div>
      <EvolutionCard title={t("Evolución por agente")} series={data.dailyByAgent} data={data} color={(key, i) => agentColor(key, i)} />
    </>
  );
}

export function ProjectFull({ data, singleProject }: { data: DashboardData; singleProject: string | null }) {
  const rows = data.branches ?? data.projects;
  const color = rowColor(rows);
  const columns = useMemo((): Column<BreakdownRow>[] => [
    { header: t(singleProject ? "Rama" : "Proyecto"), cell: (r) => <span className="with-dot"><i style={{ background: color(r) }} />{r.label}</span> },
    { header: t("Coste"), cell: (r) => cost(r.costUsd), align: "right", className: "cost" },
    { header: t("$/sesión"), cell: (r) => cost(r.sessions ? r.costUsd / r.sessions : 0), align: "right", width: "76px" },
    { header: t("Sesiones"), cell: (r) => fmt.int(r.sessions), align: "right", className: "secondary" },
    { header: t("Llamadas"), cell: (r) => fmt.int(r.calls), align: "right", className: "secondary" },
    { header: t("Overhead"), cell: (r) => (r.overheadTokens ? fmt.compact(r.overheadTokens) : "–"), align: "right", width: "76px", className: "accent" },
    barColumn(t("Reparto del coste"), rows, (r) => r.costUsd, "var(--series-blue)"),
  ], [rows, singleProject]);
  return (
    <>
      <Card title={t("Tabla")} subtitle={t(singleProject ? "{n} ramas · overhead = tokens de contexto con los que arranca cada sesión" : "{n} proyectos · overhead = tokens de contexto con los que arranca cada sesión", { n: rows.length })}>
        <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} />
      </Card>
      <div className="grid-2">
        <ShareCard title={t("Reparto del coste")} rows={rows} label={(r) => r.label} color={color} metric={M.cost} />
        <RankingCard title={t("Ranking")} rows={rows} label={(r) => r.label} color={color} metrics={[M.cost, M.avgSession, M.sessions, M.calls]} othersLabel={t(singleProject ? "Otras ramas" : "Otros proyectos")} />
      </div>
      <EvolutionCard title={t(singleProject ? "Evolución por rama" : "Evolución por proyecto")} series={data.dailyByProject} data={data} color={(key) => color({ key })} />
    </>
  );
}

export function ActivityFull({ data }: { data: DashboardData }) {
  const rows = data.activity.activities;
  const [metric, setMetric] = useState<"cost" | "turns">("cost");
  const get = (r: { costUsd: number; turns: number }) => (metric === "cost" ? r.costUsd : r.turns);
  const format = metric === "cost" ? cost : fmt.int;
  const columns = useMemo((): Column<ActivityRow>[] => [
    { header: t("Actividad"), cell: (r) => <span className="with-dot"><i style={{ background: activityColor(r.key) }} />{activityLabel(r.key)}</span> },
    { header: t("Coste"), cell: (r) => cost(r.costUsd), align: "right", className: "cost" },
    { header: t("Turnos"), cell: (r) => fmt.int(r.turns), align: "right", width: "60px" },
    { header: t("$/turno"), cell: (r) => cost(r.turns ? r.costUsd / r.turns : 0), align: "right", width: "72px", className: "secondary" },
    { header: t("Con edición"), cell: (r) => (r.editTurns ? fmt.int(r.editTurns) : "–"), align: "right", width: "84px", className: "secondary" },
    { header: "1-shot", cell: (r) => shot(r.oneShot), align: "right", width: "60px", className: (r) => shotClass(r.oneShot) },
    barColumn(t("Reparto del coste"), rows, (r) => r.costUsd, "var(--act-coding)"),
  ], [rows, metric]);
  const points = useMemo(() => {
    const b = bucketing(data.daily, data.filter);
    const byDay = stacksByBucket(
      data.activityDaily,
      (d) => b.key(d.ts),
      (d) => d.activity,
      (d) => activityLabel(d.activity),
      (d) => get({ costUsd: d.costUsd, turns: d.turns }),
      (d) => activityColor(d.activity),
    );
    return dayPoints(data.daily, data.filter).map((d) => {
      const stack = (byDay.get(d.ts) ?? []).sort((a, b) => b.value - a.value);
      const value = stack.reduce((a, s) => a + s.value, 0);
      return {
        ts: d.ts,
        value,
        stack,
        tooltip: (
          <>
            <b>{b.label(d.ts)}</b>
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
    // `get` y `format` dependen solo de la métrica.
  }, [data, metric]);
  const shots = rows.filter((r) => r.oneShot != null).map((r) => ({ key: r.key, label: activityLabel(r.key), value: r.oneShot!, valueLabel: fmt.pct(r.oneShot!), color: activityColor(r.key) }));
  const segments = rows.map((r) => ({ key: r.key, label: activityLabel(r.key), value: r.costUsd, color: activityColor(r.key) }));
  const total = rows.reduce((a, r) => a + r.costUsd, 0);
  return (
    <>
      <Card title={t("Tabla")} subtitle={t("un turno = un prompt tuyo y todo lo que hace el agente hasta el siguiente")}>
        <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} />
      </Card>
      <Card
        title={t("Evolución por actividad")}
        subtitle={t("por día, apilado")}
        actions={<Segmented value={metric} options={[{ value: "cost", label: t("Coste") }, { value: "turns", label: t("Turnos") }]} onChange={setMetric} label={t("Métrica")} />}
      >
        <Columns points={points} format={format} height={240} />
        <Legend items={rows.slice(0, 8).map((r) => ({ label: activityLabel(r.key), color: activityColor(r.key) }))} />
      </Card>
      <div className="grid-2">
        <Card title={t("Reparto del coste")} subtitle={t("{v} en total", { v: cost(total) })}>
          <div className="share-card">
            <ShareBar segments={segments} format={cost} />
          </div>
        </Card>
        <Card title={t("1-shot por actividad")} subtitle={t("solo actividades con ediciones")}>
          {shots.length ? <Bars items={shots} thick labelWidth={140} /> : <p className="empty">{t("Ningún turno con ediciones en este periodo")}</p>}
          <p className="muted small">{t("Un turno es 1-shot cuando ninguna edición falla y ningún archivo se edita dos veces.")}</p>
        </Card>
      </div>
    </>
  );
}

export function ModelFull({ data }: { data: DashboardData }) {
  const rows = data.models;
  const color = rowColor(rows);
  const oneShot = new Map(data.activity.models.map((m) => [m.model, m.oneShot]));
  const columns = useMemo((): Column<BreakdownRow>[] => [
    { header: t("Modelo"), cell: (r) => <span className="with-dot"><i style={{ background: color(r) }} />{modelName(r.key)}</span> },
    { header: t("Coste"), cell: (r) => cost(r.costUsd), align: "right", className: "cost" },
    { header: t("Llamadas"), cell: (r) => fmt.int(r.calls), align: "right" },
    { header: t("Sesiones"), cell: (r) => fmt.int(r.sessions), align: "right", className: "secondary" },
    { header: t("Cache hit"), cell: (r) => (r.cacheHit ? fmt.pct(r.cacheHit) : "–"), align: "right", width: "72px", className: "secondary" },
    { header: "1-shot", cell: (r) => shot(oneShot.get(r.key)), align: "right", width: "60px", className: (r) => shotClass(oneShot.get(r.key)) },
    barColumn(t("Reparto del coste"), rows, (r) => r.costUsd, "var(--series-violet)"),
  ], [data]);
  return (
    <>
      <Card title={t("Tabla")} subtitle={t("{n} modelos · 1-shot solo en modelos con turnos de edición", { n: rows.length })}>
        <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} />
      </Card>
      <div className="grid-2">
        <ShareCard title={t("Reparto del coste")} rows={rows} label={(r) => modelName(r.key)} color={color} metric={M.cost} />
        <RankingCard title={t("Ranking")} rows={rows} label={(r) => modelName(r.key)} color={color} metrics={[M.cost, M.calls, M.cacheHit, M.sessions]} othersLabel={t("Otros modelos")} />
      </div>
      <EvolutionCard title={t("Evolución por modelo")} series={data.dailyByModel} data={data} color={(key) => color({ key })} label={(s) => modelName(s.key)} />
    </>
  );
}

function UsesFull({ rows, header, plural, others, color, series, data, seriesTitle, mono = false, hint }: { rows: BreakdownRow[]; header: string; plural: string; others: string; color: string; series?: SeriesPoint[]; data: DashboardData; seriesTitle?: string; mono?: boolean; hint?: string }) {
  const rc = rowColor(rows);
  const columns = useMemo((): Column<BreakdownRow>[] => [
    { header: t(header), cell: (r) => r.label, className: mono ? "mono" : "" },
    { header: t("Llamadas"), cell: (r) => fmt.int(r.calls), align: "right" },
    { header: t("Errores"), cell: (r) => fmt.int(r.errors), align: "right", width: "64px", className: "secondary" },
    { header: t("% error"), cell: err, align: "right", width: "64px", className: errClass },
    barColumn(t("Uso"), rows, (r) => r.calls, color),
  ], [rows, header, plural, others, color, series, data, seriesTitle, mono, hint]);
  return (
    <>
      <Card title={t("Tabla")} subtitle={`${t(plural, { n: rows.length })}${hint ? ` · ${t(hint)}` : ""}`}>
        <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} />
      </Card>
      <div className="grid-2">
        <RankingCard title={t("Ranking")} rows={rows} label={(r) => r.label} color={() => color} metrics={[M.uses, M.errorRate]} othersLabel={t(others)} />
        <ShareCard title={t("Reparto de las llamadas")} rows={rows} label={(r) => r.label} color={rc} metric={M.uses} othersLabel={t(others)} />
      </div>
      {series && seriesTitle && <EvolutionCard title={t(seriesTitle)} series={series} data={data} color={(key) => rc({ key })} metrics={[{ value: "calls", label: "Llamadas" }]} />}
    </>
  );
}

export const ToolsFull = ({ data }: { data: DashboardData }) => (
  <UsesFull rows={data.tools} header="Herramienta" plural="{n} herramientas" others="Otras herramientas" color="var(--series-green)" series={data.dailyByTool} seriesTitle="Evolución por herramienta" data={data} hint="sin las herramientas MCP" />
);
export const ShellFull = ({ data }: { data: DashboardData }) => (
  <UsesFull rows={data.commands} header="Comando" plural="{n} comandos" others="Otros comandos" color="var(--series-yellow)" data={data} mono hint="primera palabra de cada comando, separando tuberías y encadenamientos" />
);
export const McpFull = ({ data }: { data: DashboardData }) => <UsesFull rows={data.mcp} header="Servidor" plural="{n} servidores" others="Otros servidores" color="var(--series-magenta)" data={data} />;

function CostUsesFull({ rows, header, usesHeader, color, hint, othersLabel = "Otros", agentName }: { rows: BreakdownRow[]; header: string; usesHeader: string; color: string; hint?: string; othersLabel?: string; agentName?: (id: string) => string }) {
  const rc = rowColor(rows);
  const usesMetric = { ...M.uses, label: usesHeader };
  // Con filas de varios agentes (tipos de subagente), cada fila dice de qué agente es.
  const withAgent = agentName && rows.some((r) => r.agent);
  const kind = (r: BreakdownRow) => (r.label === "(sin tipo)" ? t(r.label) : r.label);
  const label = (r: BreakdownRow) => (withAgent && r.agent ? `${kind(r)} · ${agentName!(r.agent)}` : r.label);
  const columns = useMemo((): Column<BreakdownRow>[] => [
    { header: t(header), cell: (r) => <span className="with-dot"><i style={{ background: rc(r) }} />{withAgent ? kind(r) : r.label}</span> },
    ...(withAgent
      ? [{ header: t("Agente"), cell: (r: BreakdownRow) => (r.agent ? <AgentTag id={r.agent} name={agentName!(r.agent)} /> : "–"), width: "minmax(120px, 0.7fr)" }]
      : []),
    { header: t(usesHeader), cell: (r) => fmt.int(r.calls), align: "right" },
    { header: t("Coste"), cell: (r) => cost(r.costUsd), align: "right", className: "cost" },
    { header: t("$/uso"), cell: (r) => cost(r.calls ? r.costUsd / r.calls : 0), align: "right", width: "72px", className: "secondary" },
    barColumn(t("Reparto del coste"), rows, (r) => r.costUsd, color),
  ], [rows, header, usesHeader, color, hint, othersLabel, agentName]);
  return (
    <>
      <Card title={t("Tabla")} subtitle={hint && t(hint)}>
        <DataTable rows={rows} rowKey={(r) => r.key} columns={columns} />
      </Card>
      <div className="grid-2">
        <ShareCard title={t("Reparto del coste")} rows={rows} label={label} color={rc} metric={M.cost} />
        <RankingCard title={t("Ranking")} rows={rows} label={label} color={rc} metrics={[M.cost, usesMetric]} othersLabel={t(othersLabel)} />
      </div>
    </>
  );
}

/** Nombre y color de un agente a partir de su id. */
export const AgentTag = ({ id, name }: { id: string; name: string }) => (
  <span className="with-dot">
    <i style={{ background: agentColor(id) }} />
    {name}
  </span>
);

/** Nombre de agente por id, con los agentes del panel (o el id si no está). */
export const agentNamer = (data: DashboardData) => {
  const names = new Map(data.agents.map((a) => [a.key, a.label]));
  return (id: string) => names.get(id) ?? id;
};

export const SkillsFull = ({ data }: { data: DashboardData }) => (
  <CostUsesFull rows={data.skills} header="Skill / agente" usesHeader="Usos" color="var(--series-violet)" hint="coste de las respuestas del modelo que los invocaron" othersLabel="Otras skills y agentes" />
);
export const AgentTypesFull = ({ data }: { data: DashboardData }) => (
  <CostUsesFull rows={data.agentTypes} header="Tipo" usesHeader="Llamadas" color="var(--series-blue)" hint="tipo = el subagente que eligió el agente al delegar una tarea (general, explore, code-reviewer…)" othersLabel="Otros tipos" agentName={agentNamer(data)} />
);
