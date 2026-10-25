import { useMemo } from "react";
import { t } from "../lib/i18n";
import { activityColor, activityLabel, agentColor, fmt } from "../lib/format";
import { PERIODS, projectMonth, type Period } from "../lib/period";
import type { SectionId } from "../lib/sections";
import type { DashboardData } from "../lib/useData";
import { Kpis, type Kpi } from "../components/Kpis";
import { ppDelta, relDelta } from "../lib/delta";
import { Panel } from "../components/Panel";
import { Columns, Legend, ShareBar } from "../components/Charts";
import { bucketing, dayPoints, ModelPanel } from "./panels";
import { TodayLive } from "./TodayLive";
import { rankKeys, stacksByBucket } from "./derive";


export function periodLabel(period: Period) {
  return t(PERIODS.find((p) => p.kind === period.kind)?.label ?? "").toLowerCase();
}

export function summaryKpis(data: DashboardData, budget: number | null): Kpi[] {
  const { summary: s } = data;
  const p = data.prev?.summary;
  const monthSpent = data.month.reduce((a, p) => a + p.costUsd, 0);
  const projection = projectMonth(monthSpent);
  return [
    { label: t("Coste"), value: fmt.usd(s.costUsd), hint: t("{n} llamadas", { n: fmt.int(s.calls) }), tone: "accent", delta: p && relDelta(s.costUsd, p.costUsd, false) },
    { label: t("Sesiones"), value: fmt.int(s.sessions), hint: s.sessions ? t("{v} por sesión", { v: fmt.usd(s.costUsd / s.sessions) }) : "", delta: p && relDelta(s.sessions, p.sessions, null) },
    { label: t("Cache hit"), value: fmt.pct(s.cacheHit), hint: t("{r} leídos · {w} escritos", { r: fmt.compact(s.cacheRead), w: fmt.compact(s.cacheWrite) }), delta: p && p.calls > 0 && s.calls > 0 ? ppDelta(s.cacheHit, p.cacheHit, true) : null },
    { label: t("Ahorro por caché"), value: fmt.usd(s.cacheSavingsUsd), hint: t("estimado: esa entrada a precio normal"), tone: "good", delta: p && relDelta(s.cacheSavingsUsd, p.cacheSavingsUsd, true) },
    { label: t("Burn rate"), value: `${fmt.usd(s.burnRateUsdH)}/h`, hint: t("últimos 60 minutos") },
    budget
      ? {
          label: t("Mes · {p} de {b}", { p: fmt.pct(monthSpent / budget), b: fmt.usd(budget) }),
          value: fmt.usd(monthSpent),
          hint: t(projection > budget ? "proyección {v} · supera el presupuesto" : "proyección {v} · dentro del presupuesto", { v: fmt.usd(projection) }),
          tone: projection > budget ? "warn" : undefined,
        }
      : { label: t("Gasto del mes"), value: fmt.usd(monthSpent), hint: t("proyección {v}", { v: fmt.usd(projection) }) },
  ];
}

/** Coste por día apilado por agente, con los días sin actividad a cero. */
export function DailyByAgent({ data, height = 250 }: { data: DashboardData; height?: number }) {
  const { legend, points, prev } = useMemo(() => {
    const order = data.agents.map((a) => a.key);
    const color = (key: string) => agentColor(key, Math.max(order.indexOf(key), 0));
    const b = bucketing(data.daily, data.filter);
    const byDay = stacksByBucket(data.dailyByAgent, (s) => b.key(s.ts), (s) => s.key, (s) => s.label, (s) => s.costUsd, (s) => color(s.key));
    // Leyenda: los agentes que aparecen en el gráfico, con su total del periodo.
    const legend = rankKeys(data.dailyByAgent, (s) => s.costUsd, (s) => s.label).map((x) => ({ label: `${x.label} · ${fmt.usd(x.value)}`, color: color(x.key) }));
    // Periodo anterior alineado día a día (el día i del periodo con el día i del anterior).
    const prev = data.prev ? dayPoints(data.prev.daily, data.prev.filter).map((d) => d.value) : undefined;
    const points = dayPoints(data.daily, data.filter).map((d, i) => {
      const stack = (byDay.get(d.ts) ?? []).sort((a, b) => order.indexOf(a.key) - order.indexOf(b.key));
      const value = stack.reduce((a, x) => a + x.value, 0);
      return {
        ts: d.ts,
        value,
        stack,
        tooltip: (
          <>
            <b>{b.label(d.ts)}</b>
            <div>{fmt.usd(value)}</div>
            {stack.map((x) => (
              <div key={x.key} className="muted">
                {x.label}: {fmt.usd(x.value)}
              </div>
            ))}
            {prev && prev[i] != null && <div className="muted">{t("Periodo anterior: {v}", { v: fmt.usd(prev[i]) })}</div>}
          </>
        ),
      };
    });
    return { legend, points, prev };
  }, [data]);
  return (
    <div className="chart-box">
      <Legend items={legend} />
      <Columns points={points} format={fmt.usd} height={height} compare={prev} yAxis />
    </div>
  );
}

/** Top proyectos (o ramas) de la portada: nombre, coste y variación, con una barra debajo. */
function ProjectList({ data }: { data: DashboardData }) {
  const rows = (data.branches ?? data.projects).slice(0, 6);
  if (!rows.length) return <p className="empty">{t("Sin datos en este periodo")}</p>;
  const max = Math.max(...rows.map((r) => r.costUsd), 1e-12);
  const prev = data.prev ? new Map(data.prev.projects.map((r) => [r.key, r.costUsd])) : null;
  return (
    <ul className="rank-list">
      {rows.map((r) => {
        const d = prev ? relDelta(r.costUsd, prev.get(r.key) ?? 0, false) : null;
        return (
          <li key={r.key}>
            <div className="rank-row">
              <span className="rank-name" title={r.label}>
                {r.label}
              </span>
              <span className="num cost">{fmt.usd(r.costUsd)}</span>
              {prev && <span className={`num rank-delta ${d?.tone ?? "neutral"}`}>{d ? t(d.text) : "–"}</span>}
            </div>
            <div className="rank-bar">
              <div style={{ width: `${Math.max(1.5, (r.costUsd / max) * 100)}%` }} />
            </div>
          </li>
        );
      })}
    </ul>
  );
}

/** Con el periodo «Hoy», una barra por hora (un solo día no da para un gráfico diario). */
function TodayByHour({ data }: { data: DashboardData }) {
  const hours = Array.from({ length: 24 }, (_, h) => ({ ts: h, value: 0, calls: 0 }));
  for (const p of data.hourly) {
    const h = new Date(p.ts).getHours();
    hours[h].value += p.costUsd;
    hours[h].calls += p.calls;
  }
  const points = hours.map((x) => ({
    ...x,
    tooltip: (
      <>
        <b>
          {String(x.ts).padStart(2, "0")}:00 – {String((x.ts + 1) % 24).padStart(2, "0")}:00
        </b>
        <div>{fmt.usd(x.value)}</div>
        <div className="muted">{t("{n} llamadas", { n: fmt.int(x.calls) })}</div>
      </>
    ),
  }));
  return (
    <div className="chart-box">
      <Columns points={points} format={fmt.usd} height={250} axis={(h) => `${String(h).padStart(2, "0")}h`} />
    </div>
  );
}

/** Reparto del coste por actividad, en barra al 100 %. */
function ActivityShare({ data }: { data: DashboardData }) {
  const rows = data.activity.activities;
  const segments = rows.map((r) => ({ key: r.key, label: activityLabel(r.key), value: r.costUsd, color: activityColor(r.key) }));
  const edits = rows.reduce((a, r) => a + r.editTurns, 0);
  const ok = rows.reduce((a, r) => a + (r.oneShot ?? 0) * r.editTurns, 0);
  return (
    <>
      <ShareBar segments={segments} format={fmt.usd} limit={8} compact />
      {edits > 0 && (
        <div className="panel-note">
          <span>{t("1-shot global")}</span>
          <span className={`num ${ok / edits >= 0.95 ? "good" : ""}`}>{fmt.pct(ok / edits)}</span>
        </div>
      )}
    </>
  );
}

export function Overview({
  data,
  period,
  budget,
  singleProject,
  open,
  openSession,
}: {
  data: DashboardData;
  period: Period;
  budget: number | null;
  singleProject: string | null;
  open: (s: SectionId) => void;
  openSession: (id: string) => void;
}) {
  const scope = singleProject ? t("proyecto {name}", { name: singleProject }) : t("todos los agentes y proyectos");
  const monthSpent = data.month.reduce((a, p) => a + p.costUsd, 0);
  const over = budget != null && projectMonth(monthSpent) > budget;
  const props = { data, full: false, singleProject };
  return (
    <div className="main">
      <header className="page-head">
        <div className="page-title-row">
          <h1>
            {t("Resumen")} <span className="muted">· {periodLabel(period)} · {scope}</span>
          </h1>
          {data.prev?.filter.from != null && data.prev.filter.to != null && (
            <span className="compare-pill">
              {period.kind === "today"
                ? t("Comparando con ayer ({d})", { d: fmt.day(data.prev.filter.from) })
                : t("Comparando con {a} – {b}", { a: fmt.day(data.prev.filter.from), b: fmt.day(data.prev.filter.to - 1) })}
            </span>
          )}
        </div>
      </header>
      {data.summary.calls === 0 && (
        <div className="notice">{t('No hay llamadas en este periodo. Si acabas de instalar la app, espera a que termine el escaneo inicial o elige "Todo".')}</div>
      )}
      <Kpis items={summaryKpis(data, budget)} />
      {over && <div className="notice warn">{t("⚠ La proyección del mes supera el presupuesto de {b}.", { b: fmt.usd(budget!) })}</div>}
      <div className="grid-top fixed-row">
        {period.kind === "today" ? (
          <Panel id="daily" title={t("Gasto de hoy")} question={t("por hora")} onOpen={() => open("daily")} openLabel={t("Daily Activity ›")}>
            <TodayByHour data={data} />
          </Panel>
        ) : (
          <Panel id="daily" title={t("Gasto diario")} question={data.prev ? t("por agente · línea discontinua = periodo anterior") : t("por agente")} onOpen={() => open("daily")} openLabel={t("Daily Activity ›")}>
            <DailyByAgent data={data} />
          </Panel>
        )}
        <Panel id="today" title={t("Hoy en directo")} question={t("sea cual sea el periodo")}>
          <TodayLive filter={data.filter} openSession={openSession} />
        </Panel>
      </div>
      <div className="grid-3">
        <Panel id="project" title={singleProject ? t("Top ramas") : t("Top proyectos")} onOpen={() => open("project")} openLabel={t("By Project ›")}>
          <ProjectList data={data} />
        </Panel>
        <Panel id="model" title={t("Top modelos")} onOpen={() => open("model")} openLabel={t("By Model ›")}>
          <ModelPanel {...props} />
          {data.summary.unpricedModels.length > 0 && (
            <button className="card-alert" onClick={() => open("pricing")}>
              <span>
                {t(data.summary.unpricedModels.length === 1 ? "1 modelo sin precio: sus llamadas cuentan como $0" : "{n} modelos sin precio: sus llamadas cuentan como $0", {
                  n: data.summary.unpricedModels.length,
                })}
              </span>
              <span>{t("Añadir ›")}</span>
            </button>
          )}
        </Panel>
        <Panel id="activity" title={t("En qué se va el gasto")} onOpen={() => open("activity")} openLabel={t("By Activity ›")}>
          <ActivityShare data={data} />
        </Panel>
      </div>
    </div>
  );
}
