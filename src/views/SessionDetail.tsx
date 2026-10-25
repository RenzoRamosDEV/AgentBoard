import { useEffect, useMemo, useState } from "react";
import { api, type AfterCompaction, type KeyCost, type SessionDetail as Detail, type ToolStat, type TurnRow } from "../lib/api";
import { createPager, type PagerState } from "../lib/pager";
import { activityColor, activityLabel, agentColor, fmt, modelName } from "../lib/format";
import { t } from "../lib/i18n";
import { Bars, LineChart } from "../components/Charts";
import { DataTable, type Column } from "../components/DataTable";
import { Kpis, type Kpi } from "../components/Kpis";
import { Empty } from "../components/Panel";

/** Posición en la línea de tiempo (una entrada por llamada) de la primera llamada tras `ts`. */
export function markerIndex(timeline: { ts: number }[], ts: number): number {
  const i = timeline.findIndex((p) => p.ts >= ts);
  return i < 0 ? timeline.length - 1 : i;
}

const TOP_TOOLS = 3;

/** La línea de coste acumulado, que llega en columnas, como puntos. */
export function timelinePoints(timeline: Detail["timeline"]): { ts: number; costUsd: number }[] {
  return timeline.ts.map((ts, i) => ({ ts, costUsd: timeline.cost[i] }));
}

/** Aviso de que tras compactar cada turno cuesta bastante más (cálculo del backend). */
export function compactionAlert(after: AfterCompaction | null): string | null {
  if (!after || after.ratio == null || after.ratio < 1.2) return null;
  return t("Tras la {k}.ª compactación, cada turno cuesta {x}× más. Buen punto para abrir una sesión nueva.", { k: after.k, x: after.ratio.toFixed(1) });
}

/** Turnos por página. */
const TURNS_PAGE = 30;

export function SessionDetail({ id, refresh, back, backLabel }: { id: string; refresh: number; back: () => void; backLabel?: string }) {
  const [d, setD] = useState<Detail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [turns, setTurns] = useState<PagerState<TurnRow>>({ rows: [], total: 0 });
  // Turnos por páginas, en orden; un aviso de datos cambiados vuelve a la primera.
  const pager = useMemo(() => createPager<TurnRow, null>((offset, limit) => api.sessionTurns(id, { offset, limit }), TURNS_PAGE, setTurns), [id]);

  useEffect(() => {
    let alive = true;
    api
      .sessionDetail(id)
      .then((x) => alive && (setD(x), setError(null)))
      .catch((e) => alive && setError(String(e)));
    pager.load(null);
    return () => {
      alive = false;
    };
  }, [id, refresh, pager]);

  if (error) return <div className="main error">{t("No se pudieron cargar los datos: {e}", { e: error })}</div>;
  if (!d) return <div className="main muted">{t("Cargando…")}</div>;

  const s = d.session;
  const alert = compactionAlert(d.afterCompaction);
  const timeline = timelinePoints(d.timeline);
  const toolCalls = d.tools.reduce((a, x) => a + x.calls, 0);
  const toolErrors = d.tools.reduce((a, x) => a + x.errors, 0);
  const kpis: Kpi[] = [
    { label: t("Coste"), value: s.hasPrice || s.costUsd > 0 ? fmt.usd(s.costUsd) : t("sin precio"), hint: t("{n} llamadas", { n: fmt.int(s.calls) }), tone: "accent" },
    { label: t("Turnos"), value: s.turns ? fmt.int(s.turns) : "–", hint: s.turns ? t("{v} por turno", { v: fmt.usd(s.costUsd / s.turns) }) : t("este agente no registra turnos") },
    { label: t("Duración"), value: fmt.duration(s.endedAt - s.startedAt), hint: `${fmt.time(s.startedAt)} → ${fmt.time(s.endedAt)}` },
    { label: t("Cache hit"), value: fmt.pct(s.cacheHit), hint: t("{r} de entrada · {w} de salida", { r: fmt.compact(s.inputTokens), w: fmt.compact(s.outputTokens) }) },
    { label: t("Compactaciones"), value: fmt.int(d.compactions.length), hint: d.compactions.length ? d.compactions.map(fmt.time).join(" · ") : t("el contexto no llegó a llenarse"), tone: d.compactions.length >= 3 ? "warn" : undefined },
    { label: t("Herramientas"), value: fmt.int(toolCalls), hint: toolCalls ? t("{p} con error", { p: fmt.pct(toolErrors / toolCalls) }) : "" },
  ];

  const points = timeline.map((p) => ({
    ts: p.ts,
    value: p.costUsd,
    tooltip: (
      <>
        <b>{fmt.time(p.ts)}</b>
        <div>{t("Acumulado: {v}", { v: fmt.usd(p.costUsd) })}</div>
      </>
    ),
  }));
  const markers = d.compactions.map((ts, i) => ({ index: markerIndex(timeline, ts), label: t("compact. {n}", { n: i + 1 }) }));

  const turnColumns: Column<TurnRow>[] = [
    { header: "#", cell: (r) => r.n, width: "36px", className: "muted num" },
    { header: t("Hora"), cell: (r) => fmt.time(r.ts), width: "56px", className: "secondary num" },
    {
      header: t("Actividad"),
      cell: (r) => (
        <span className="with-dot">
          <i style={{ background: activityColor(r.activity) }} />
          {activityLabel(r.activity)}
        </span>
      ),
      width: "120px",
    },
    {
      header: t("Herramientas"),
      cell: (r) =>
        r.tools.length ? (
          <span className="mono small" title={r.tools.map(([n, c]) => `${n} ×${c}`).join(" · ")}>
            {r.tools
              .slice(0, TOP_TOOLS)
              .map(([n, c]) => `${n} ×${c}`)
              .join(" · ")}
            {r.tools.length > TOP_TOOLS ? " …" : ""}
          </span>
        ) : (
          <span className="muted">–</span>
        ),
      className: (r) => (r.toolErrors ? "warn" : "secondary"),
    },
    { header: t("Entrada / salida"), cell: (r) => `${fmt.compact(r.inputTokens)} / ${fmt.compact(r.outputTokens)}`, align: "right", width: "120px", className: "secondary" },
    { header: t("Coste"), cell: (r) => fmt.usd(r.costUsd), align: "right", width: "72px", className: "cost" },
  ];

  const toolColumns: Column<ToolStat>[] = [
    { header: t("Herramienta"), cell: (r) => <span title={r.tool}>{r.tool}</span>, width: "minmax(110px, 1fr)", className: "mono", sort: (r) => r.tool },
    { header: t("Usos"), cell: (r) => fmt.int(r.calls), align: "right", width: "48px", sort: (r) => r.calls },
    { header: t("Error"), cell: (r) => (r.errors ? fmt.pct(r.errors / r.calls) : "–"), align: "right", width: "56px", className: (r) => (r.errors ? "warn" : "muted"), sort: (r) => r.errors / r.calls },
    { header: "p50", cell: (r) => (r.p50Ms == null ? "–" : fmt.ms(r.p50Ms)), align: "right", width: "64px", className: "secondary", sort: (r) => r.p50Ms ?? -1 },
    { header: "p95", cell: (r) => (r.p95Ms == null ? "–" : fmt.ms(r.p95Ms)), align: "right", width: "72px", className: (r) => ((r.p95Ms ?? 0) >= 30_000 ? "warn" : "secondary"), sort: (r) => r.p95Ms ?? -1 },
  ];

  const modelColumns: Column<KeyCost>[] = [
    { header: t("Modelo"), cell: (r) => modelName(r.key) },
    { header: t("Llamadas"), cell: (r) => fmt.int(r.calls), align: "right", width: "64px" },
    { header: t("Coste"), cell: (r) => fmt.usd(r.costUsd), align: "right", width: "72px", className: "cost" },
  ];

  return (
    <div className="main">
      <header className="page-head">
        <button className="link back" onClick={back}>
          {backLabel ?? t("‹ Proyectos")}
        </button>
        <div className="session-title">
          <h1>
            {s.project ?? t("(sin proyecto)")} {s.branch && <span className="muted">· {s.branch}</span>}
          </h1>
          <span className="tag with-dot">
            <i style={{ background: agentColor(s.agentId) }} />
            {s.agentName}
          </span>
          {s.model && <span className="tag mono">{s.model}</span>}
          {s.isSubagent && <span className="tag">{t("subagente")}</span>}
          <span className="muted num session-when">
            {fmt.dateTime(s.startedAt)} → {fmt.time(s.endedAt)}
          </span>
        </div>
      </header>
      <Kpis items={kpis} />
      <div className="grid-top">
        <section className="panel">
          <header className="panel-head">
            <div className="panel-title">
              <h2>{t("Coste acumulado")}</h2>
              <span className="muted">{d.compactions.length ? t("llamada a llamada · en rojo, las compactaciones de contexto") : t("llamada a llamada")}</span>
            </div>
          </header>
          <LineChart points={points} format={fmt.usd} height={230} markers={markers} axis={fmt.time} />
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
          />
          <DataTable rows={d.models} rowKey={(r) => r.key} columns={modelColumns} collapse={false} />
          {alert && <p className="card-alert static">{alert}</p>}
        </section>
      </div>
      <div className="grid-detail">
        <section className="panel">
          <header className="panel-head">
            <div className="panel-title">
              <h2>{t("Turnos")}</h2>
              <span className="muted">{t("el texto de los prompts no se guarda")}</span>
            </div>
          </header>
          {d.turnsTotal ? (
            <DataTable rows={turns.rows} rowKey={(r) => r.id} columns={turnColumns} more={{ total: turns.total, onMore: pager.more }} />
          ) : (
            <Empty>{t("Este agente no registra turnos: el coste se reparte por llamada.")}</Empty>
          )}
        </section>
        <section className="panel">
          <header className="panel-head">
            <div className="panel-title">
              <h2>{t("Herramientas y latencia")}</h2>
            </div>
          </header>
          <DataTable rows={d.tools} rowKey={(r) => r.tool} columns={toolColumns} collapse={20} empty={t("Sin herramientas en esta sesión")} />
        </section>
      </div>
    </div>
  );
}
