import { useEffect, useRef, useState } from "react";
import { api, type Filter, type Point, type SessionRow } from "../lib/api";
import { relDelta } from "../lib/delta";
import { agentColor, fmt, modelName } from "../lib/format";
import { t } from "../lib/i18n";
import { Columns } from "../components/Charts";
import { whenVisible } from "../lib/live";
import { filterKey, useQuery } from "../lib/query";

/** Una sesión cuenta como activa si tuvo llamadas en los últimos 15 minutos. */
export const ACTIVE_MS = 15 * 60_000;
const DAY_MS = 864e5;

/** Inicio del día local de `now` y de ayer, y el mismo instante de ayer (para comparar). */
export function todayRanges(now: number) {
  const d = new Date(now);
  const today = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const yesterday = new Date(d.getFullYear(), d.getMonth(), d.getDate() - 1).getTime();
  return { today, yesterday, sameTimeYesterday: now - DAY_MS };
}

/** La última sesión que se usó: la de datos más recientes, y si sigue activa (últimos 15 min). */
export function lastUsedSession(rows: SessionRow[], now: number): { session: SessionRow; active: boolean } | null {
  const last = rows.reduce<SessionRow | null>((a, b) => (!a || b.endedAt > a.endedAt ? b : a), null);
  return last ? { session: last, active: now - last.endedAt < ACTIVE_MS } : null;
}

interface Live {
  /** Hora de la carga. */
  at: number;
  today: number;
  yesterday: number;
  burn: number;
  hours: Point[];
  last: { session: SessionRow; active: boolean } | null;
}

/** «hace 3 min», «hace 2 h»… */
function ago(ms: number) {
  const m = Math.max(0, Math.round(ms / 60_000));
  if (m < 1) return t("ahora mismo");
  if (m < 60) return t("hace {n} min", { n: m });
  return t("hace {n} h", { n: Math.round(m / 60) });
}

/** Tarjeta «Hoy en directo»: lo de hoy, siempre, sea cual sea el periodo elegido. */
export function TodayLive({ filter, openSession }: { filter: Filter; openSession: (id: string) => void }) {
  const [now, setNow] = useState(Date.now());
  // Recarga periódica: sin datos nuevos también caducan «Activa ahora», el ritmo de la
  // última hora y el día al cruzar la medianoche. Con la ventana oculta no se consulta nada.
  const [tick, setTick] = useState(0);
  // Solo agentes y proyectos: el periodo de la barra no afecta a esta tarjeta.
  const scope = { agents: filter.agents, projects: filter.projects };

  useEffect(() => {
    const id = setInterval(
      () =>
        whenVisible(document, () => {
          setNow(Date.now());
          setTick((n) => n + 1);
        }),
      30_000,
    );
    return () => clearInterval(id);
  }, []);

  const q = useQuery<Live>("today_live", `${filterKey(scope)}|${tick}`, async () => {
    const at = Date.now();
    const r = todayRanges(at);
    const today: Filter = { ...scope, from: r.today };
    const [s, y, hours, sessions] = await Promise.all([
      api.summary(today),
      api.summary({ ...scope, from: r.yesterday, to: r.sameTimeYesterday }),
      api.timeseries(today, "hour"),
      api.sessions(today, { limit: 200 }),
    ]);
    // Sin sesiones hoy, la última usada de los últimos 30 días.
    const rows = sessions.rows.length ? sessions.rows : (await api.sessions({ ...scope, from: at - 30 * DAY_MS }, { limit: 200 })).rows;
    return { at, today: s.costUsd, yesterday: y.costUsd, burn: s.burnRateUsdH, hours, last: lastUsedSession(rows, at) };
  });
  // Con cada recarga periódica cambia la clave: mientras llega, se sigue viendo la anterior.
  const shown = useRef<Live | null>(null);
  if (q.data) shown.current = q.data;
  const live = q.error ? null : shown.current;
  const clock = live ? Math.max(now, live.at) : now;

  if (!live) return <p className="empty">{t("Cargando…")}</p>;

  const delta = relDelta(live.today, live.yesterday, false);
  const byHour = Array.from({ length: 24 }, (_, h) => ({ ts: h, value: 0 }));
  for (const p of live.hours) byHour[new Date(p.ts).getHours()].value += p.costUsd;
  const l = live.last;
  // La actividad caduca con el reloj, no con la última carga.
  const isActive = !!l && clock - l.session.endedAt < ACTIVE_MS;

  return (
    <div className="today-live">
      <div className="today-head">
        <strong className="today-value num">{fmt.usd(live.today)}</strong>
        {delta && (
          <span className={`kpi-delta num ${delta.tone}`} title={t("frente a ayer hasta la misma hora ({v})", { v: fmt.usd(live.yesterday) })}>
            {t(delta.text)} {t("vs. ayer")}
          </span>
        )}
      </div>
      <div className="today-row">
        <span className="muted">{t("Ritmo (última hora)")}</span>
        <span className="num">{fmt.usd(live.burn)}/h</span>
      </div>
      <div className="today-chart">
        <Columns points={byHour} format={fmt.usd} height={120} yAxis={false} axis={(h) => `${String(h).padStart(2, "0")}h`} />
      </div>
      <div className={`today-session ${isActive ? "active" : ""}`}>
        {l ? (
          <>
            <div className="today-session-head">
              <span className={`status-dot ${isActive ? "on" : ""}`} />
              <span>{isActive ? t("Activa ahora") : t("Última sesión · {ago}", { ago: ago(clock - l.session.endedAt) })}</span>
            </div>
            <button className="today-session-row" onClick={() => openSession(l.session.id)} title={l.session.model ? `${modelName(l.session.model)} · ${t("Ver la sesión ›")}` : t("Ver la sesión ›")}>
              <span className="today-session-main">
                <span className="today-session-name">
                  {l.session.project ?? t("(sin proyecto)")}
                  {l.session.branch && <span className="muted"> · {l.session.branch}</span>}
                </span>
                <span className="with-dot muted small">
                  <i style={{ background: agentColor(l.session.agentId) }} />
                  {l.session.agentName} · {t("empezó {ago}", { ago: ago(clock - l.session.startedAt) })}
                </span>
              </span>
              <span className="num cost">{fmt.usd(l.session.costUsd)}</span>
              <span className="today-session-go" aria-hidden>
                ›
              </span>
            </button>
          </>
        ) : (
          <span className="muted">{t("Aún no hay sesiones.")}</span>
        )}
      </div>
    </div>
  );
}
