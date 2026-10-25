import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Empty } from "./Panel";
import { useTooltip } from "./Tooltip";
import { getLang, LOCALES, t } from "../lib/i18n";
import { fmt } from "../lib/format";

export interface BarItem {
  key: string;
  label: string;
  value: number;
  valueLabel: string;
  color?: string;
  tooltip?: ReactNode;
}

/** Barras horizontales: etiqueta, barra proporcional al máximo y valor. */
export function Bars({ items, color = "var(--series-1)", limit, labelWidth = 84, thick = false }: { items: BarItem[]; color?: string; limit?: number; labelWidth?: number; thick?: boolean }) {
  const setTip = useTooltip();
  if (!items.length) return <Empty />;
  const shown = limit ? items.slice(0, limit) : items;
  const max = Math.max(...shown.map((i) => i.value), 1e-12);
  return (
    <div className={`bars ${thick ? "bars-thick" : ""}`}>
      {shown.map((i) => (
        <div
          key={i.key}
          className="bar-row"
          style={{ "--bar-label": `${labelWidth}px` } as React.CSSProperties}
          onMouseMove={i.tooltip ? (e) => setTip({ x: e.clientX, y: e.clientY, content: i.tooltip }) : undefined}
          onMouseLeave={i.tooltip ? () => setTip(null) : undefined}
        >
          <span className="bar-label" title={i.label}>
            {i.label}
          </span>
          <div className="bar-track">
            <div className="bar-fill" style={{ width: `${Math.max((i.value / max) * 100, i.value > 0 ? 1.5 : 0)}%`, background: i.color ?? color }} />
          </div>
          <span className="bar-value num">{i.valueLabel}</span>
        </div>
      ))}
    </div>
  );
}

/** Barra dentro de una fila de tabla: proporcional al máximo del conjunto, sin etiqueta ni cifra. */
export const InlineBar = ({ value, max, color = "var(--series-1)" }: { value: number; max: number; color?: string }) => (
  <span className="inline-bar" aria-hidden>
    <span className="inline-bar-fill" style={{ width: `${max > 0 ? Math.max((value / max) * 100, value > 0 ? 1.5 : 0) : 0}%`, background: color }} />
  </span>
);

export interface ColumnPoint {
  ts: number;
  value: number;
  /** Series apiladas (de abajo arriba); si falta se usa `value`. */
  stack?: { key: string; value: number; color: string; label: string }[];
  tooltip?: ReactNode;
}

function useWidth<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [w, setW] = useState(400);
  useLayoutEffect(() => {
    if (!ref.current) return;
    const ro = new ResizeObserver(([e]) => setW(e.contentRect.width));
    ro.observe(ref.current);
    return () => ro.disconnect();
  }, []);
  return [ref, w] as const;
}

/**
 * Formato de las marcas de un eje: cortas para que quepan en su margen. Los importes pasan a
 * «$250» / «$1.5K» y las cantidades a «1.5K»; `format` completo sigue en tooltips y en «máx.».
 */
export function tickFormat(format: (v: number) => string): (v: number) => string {
  return format(1234.5).includes("$") ? fmt.usdAxis : (v: number) => (v >= 1000 ? fmt.compact(v) : format(v));
}

/** Paso «redondo» (1, 2, 2.5, 5 × 10ⁿ) mayor o igual que `raw`, para las marcas de un eje. */
export function niceStep(raw: number): number {
  if (!(raw > 0)) return 1;
  const exp = Math.pow(10, Math.floor(Math.log10(raw)));
  const f = raw / exp;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 2.5 ? 2.5 : f <= 5 ? 5 : 10) * exp;
}

/** Gráfico de columnas por día (o apilado por serie), con eje de fechas y tooltip. */
export function Columns({
  points,
  height = 190,
  color = "var(--accent)",
  format,
  axis = (ts: number) => new Date(ts).toLocaleDateString(LOCALES[getLang()], { day: "numeric", month: "short" }),
  compare,
  yAxis = true,
}: {
  points: ColumnPoint[];
  height?: number;
  color?: string;
  format: (v: number) => string;
  axis?: (ts: number) => string;
  /** Valores de otro periodo alineados con `points`, pintados como línea discontinua. */
  compare?: number[];
  /** Eje de valores a la izquierda con líneas guía (por defecto, sí). */
  yAxis?: boolean;
}) {
  const setTip = useTooltip();
  const [ref] = useWidth<HTMLDivElement>();
  if (!points.length) return <Empty />;
  const rawMax = Math.max(...points.map((p) => p.value), ...(compare ?? []), 1e-12);
  // Con eje, la escala llega a un múltiplo «redondo» para que las marcas sean legibles.
  const step = yAxis ? niceStep(rawMax / 3) : 0;
  const tick = tickFormat(format);
  const max = yAxis ? Math.max(step * Math.ceil(rawMax / step), 1e-12) : rawMax;
  const px = (v: number) => Math.round((v / max) * height);
  const n = points.length;
  const peak = points.reduce((a, b) => (b.value > a.value ? b : a), points[0]);
  const line = compare?.length
    ? compare
        .slice(0, n)
        .map((v, i) => `${(i + 0.5).toFixed(2)},${(height - (v / max) * height).toFixed(1)}`)
        .join(" ")
    : null;
  const mid = Math.floor(n / 2);
  const template = { gridTemplateColumns: `repeat(${n}, minmax(0, 1fr))` };
  return (
    <div className="columns" ref={ref}>
      <div className="columns-max muted">{t("máx. {v} · {d}", { v: format(peak.value), d: axis(peak.ts) })}</div>
      <div className={yAxis ? "columns-y" : ""}>
      {yAxis && (
        <div className="columns-y-axis muted num" style={{ height }} aria-hidden>
          {Array.from({ length: Math.round(max / step) + 1 }, (_, i) => i * step).map((v) => (
            <span key={v} style={{ top: `${(1 - v / max) * 100}%` }}>
              {tick(v)}
            </span>
          ))}
        </div>
      )}
      <div className="columns-plot" style={{ height, ...template }}>
        {yAxis && Array.from({ length: Math.round(max / step) }, (_, i) => (i + 1) * step).map((v) => <div key={v} className="columns-grid" style={{ bottom: `${(v / max) * 100}%` }} />)}
        {points.map((p) => (
          <div
            key={p.ts}
            className="column"
            style={{ height }}
            onMouseMove={p.tooltip ? (e) => setTip({ x: e.clientX, y: e.clientY, content: p.tooltip }) : undefined}
            onMouseLeave={p.tooltip ? () => setTip(null) : undefined}
          >
            {p.stack ? (
              <div className="column-stack">
                {/* Sin los segmentos de altura 0: si no, el redondeo de arriba o abajo cae en uno invisible. */}
                {[...p.stack].reverse().filter((s) => px(s.value) > 0).map((s) => (
                  <div key={s.key} className="column-seg" style={{ height: px(s.value), background: s.color }} />
                ))}
              </div>
            ) : (
              <div className="column-seg column-single" style={{ height: px(p.value), background: color }} />
            )}
          </div>
        ))}
        {line && (
          <svg className="columns-compare" viewBox={`0 0 ${n} ${height}`} preserveAspectRatio="none" aria-hidden>
            <polyline points={line} />
          </svg>
        )}
      </div>
      </div>
      <div className={`columns-axis muted ${yAxis ? "with-y" : ""}`} style={template}>
        {points.map((p, i) => (
          <span key={p.ts} className={i === 0 ? "first" : i === n - 1 ? "last" : ""}>
            {i === 0 || i === n - 1 || (n > 6 && i === mid) ? axis(p.ts) : ""}
          </span>
        ))}
      </div>
    </div>
  );
}

/** Línea con área bajo la curva (p. ej. acumulado del periodo), con tooltip por punto. */
export function LineChart({
  points,
  height = 200,
  color = "var(--accent)",
  format,
  axis = (ts: number) => new Date(ts).toLocaleDateString(LOCALES[getLang()], { day: "numeric", month: "short" }),
  markers,
}: {
  points: { ts: number; value: number; tooltip?: ReactNode }[];
  height?: number;
  color?: string;
  format: (v: number) => string;
  axis?: (ts: number) => string;
  /** Líneas verticales en la posición de un punto (p. ej. compactaciones de contexto). */
  markers?: { index: number; label: string }[];
}) {
  const setTip = useTooltip();
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  if (!points.length) return <Empty />;
  // Margen izquierdo para el eje de valores, con marcas «redondas» (menor si hay poco sitio).
  const pad = { top: 12, right: 12, bottom: 4, left: width < 480 ? 46 : 60 };
  const w = Math.max(width, 100);
  const rawMax = Math.max(...points.map((p) => p.value), 1e-12);
  const step = niceStep(rawMax / 3);
  const tick = tickFormat(format);
  const max = Math.max(step * Math.ceil(rawMax / step), 1e-12);
  const ticks = Array.from({ length: Math.round(max / step) + 1 }, (_, i) => i * step);
  const iw = w - pad.left - pad.right;
  const ih = height - pad.top - pad.bottom;
  const x = (i: number) => pad.left + (points.length > 1 ? (i / (points.length - 1)) * iw : iw / 2);
  const y = (v: number) => pad.top + ih - (v / max) * ih;
  const path = points.map((p, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(p.value).toFixed(1)}`).join("");
  const area = `${path}L${x(points.length - 1).toFixed(1)},${(pad.top + ih).toFixed(1)}L${x(0).toFixed(1)},${(pad.top + ih).toFixed(1)}Z`;
  const mid = Math.floor(points.length / 2);
  const onMove = (e: React.MouseEvent<SVGRectElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const i = Math.round(((e.clientX - rect.left) / rect.width) * (points.length - 1));
    const idx = Math.min(points.length - 1, Math.max(0, i));
    setHover(idx);
    if (points[idx].tooltip) setTip({ x: e.clientX, y: e.clientY, content: points[idx].tooltip });
  };
  return (
    <div className="linechart" ref={ref}>
      <div className="columns-max muted">{t("máx. {v}", { v: format(rawMax) })}</div>
      <svg width={w} height={height} role="img" aria-label="Evolución">
        {ticks.map((v) => (
          <g key={v}>
            {v > 0 && <line className="grid" x1={pad.left} x2={pad.left + iw} y1={y(v)} y2={y(v)} />}
            <text className="axis-label" x={pad.left - 8} y={y(v)} textAnchor="end" dominantBaseline="middle">
              {tick(v)}
            </text>
          </g>
        ))}
        <line className="baseline" x1={pad.left} x2={pad.left + iw} y1={y(0)} y2={y(0)} />
        <path d={area} fill={color} opacity={0.12} />
        <path d={path} fill="none" stroke={color} strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
        {markers?.map((m, i) => (
          <g key={i} className="marker">
            <line x1={x(m.index)} x2={x(m.index)} y1={pad.top} y2={pad.top + ih} />
            {/* Si dos marcas caen muy juntas, solo la primera lleva texto; cerca del borde derecho, va a la izquierda. */}
            {(i === 0 || x(m.index) - x(markers[i - 1].index) >= (x(m.index) > pad.left + iw * 0.85 ? 170 : 85)) && (
            <text x={x(m.index) > pad.left + iw * 0.85 ? x(m.index) - 4 : x(m.index) + 4} y={pad.top + 10} textAnchor={x(m.index) > pad.left + iw * 0.85 ? "end" : "start"}>
              {m.label}
            </text>
            )}
          </g>
        ))}
        {hover != null && (
          <>
            <line className="crosshair" x1={x(hover)} x2={x(hover)} y1={pad.top} y2={pad.top + ih} />
            <circle cx={x(hover)} cy={y(points[hover].value)} r={4} fill={color} stroke="var(--surface-1)" strokeWidth={2} />
          </>
        )}
        <rect
          x={pad.left}
          y={pad.top}
          width={iw}
          height={ih}
          fill="transparent"
          onMouseMove={onMove}
          onMouseLeave={() => {
            setHover(null);
            setTip(null);
          }}
        />
      </svg>
      <div className="columns-axis muted with-y" style={{ gridTemplateColumns: "1fr 1fr 1fr" }}>
        <span className="first">{axis(points[0].ts)}</span>
        <span style={{ gridColumn: 2 }}>{points.length > 2 ? axis(points[mid].ts) : ""}</span>
        <span className="last">{points.length > 1 ? axis(points[points.length - 1].ts) : ""}</span>
      </div>
    </div>
  );
}

/** Selector de opciones excluyentes (métrica, desglose…). */
export function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: { value: T; label: string }[]; onChange: (v: T) => void; label?: string }) {
  return (
    <div className="segmented-group">
      {label && <span className="segmented-label">{label}</span>}
      <div className="segmented" role="group" aria-label={label}>
        {options.map((o) => (
          <button key={o.value} className={`segment ${o.value === value ? "active" : ""}`} onClick={() => onChange(o.value)}>
            {o.label}
          </button>
        ))}
      </div>
    </div>
  );
}

export interface Segment {
  key: string;
  label: string;
  value: number;
  color: string;
}

/** Anillo de reparto con total en el centro y leyenda de los mayores. */
export function Donut({ segments, center, sub, format, size = 150 }: { segments: Segment[]; center: string; sub?: string; format: (v: number) => string; size?: number }) {
  const setTip = useTooltip();
  const total = segments.reduce((a, s) => a + s.value, 0);
  if (!total) return <Empty />;
  let acc = 0;
  const arcs = segments
    .filter((s) => s.value > 0)
    .map((s) => {
      const len = (s.value / total) * 100;
      const arc = { ...s, dash: `${len.toFixed(3)} ${(100 - len).toFixed(3)}`, offset: -acc, share: s.value / total };
      acc += len;
      return arc;
    });
  return (
    <div className="donut">
      <svg width={size} height={size} viewBox="0 0 42 42" role="img" aria-label={`Reparto: ${center}`}>
        <circle cx="21" cy="21" r="15.9" fill="none" stroke="var(--track)" strokeWidth="6" />
        {arcs.map((a) => (
          <circle
            key={a.key}
            cx="21"
            cy="21"
            r="15.9"
            fill="none"
            stroke={a.color}
            strokeWidth="6"
            strokeDasharray={a.dash}
            strokeDashoffset={a.offset}
            transform="rotate(-90 21 21)"
            onMouseMove={(e) =>
              setTip({
                x: e.clientX,
                y: e.clientY,
                content: (
                  <>
                    <b>{a.label}</b>
                    <div>
                      {format(a.value)} · {Math.round(a.share * 100)}%
                    </div>
                  </>
                ),
              })
            }
            onMouseLeave={() => setTip(null)}
          />
        ))}
        <text x="21" y="20.2" textAnchor="middle" className="donut-center">
          {center}
        </text>
        {sub && (
          <text x="21" y="25.5" textAnchor="middle" className="donut-sub">
            {sub}
          </text>
        )}
      </svg>
      <div className="legend">
        {arcs.slice(0, 4).map((a) => (
          <span key={a.key}>
            <i style={{ background: a.color }} />
            {a.label} {Math.round(a.share * 100)}%
          </span>
        ))}
      </div>
    </div>
  );
}

/** Reparto: barra apilada al 100 % con segmentos separados y lista con porcentaje y valor. */
export function ShareBar({ segments, format, limit = 8, compact = false, othersLabel = t("Otros") }: { segments: Segment[]; format: (v: number) => string; limit?: number; compact?: boolean; othersLabel?: string }) {
  const setTip = useTooltip();
  const total = segments.reduce((a, s) => a + s.value, 0);
  if (!total) return <Empty />;
  const sorted = [...segments].filter((s) => s.value > 0).sort((a, b) => b.value - a.value);
  // Como mucho `limit` entradas: la última agrupa el resto.
  const shown = sorted.length > limit ? sorted.slice(0, limit - 1) : sorted;
  const tail = sorted.slice(shown.length);
  const rest = tail.reduce((a, s) => a + s.value, 0);
  const items = tail.length ? [...shown, { key: "__otros", label: `${othersLabel} (${tail.length})`, value: rest, color: "var(--text-muted)" }] : shown;
  const pct = (v: number) => v / total;
  return (
    <div className={`share ${compact ? "share-compact" : ""}`}>
      <div className="share-bar" role="img" aria-label="Reparto">
        {items.map((s) => (
          <div
            key={s.key}
            className="share-seg"
            // Crece según su % del total: con valores absolutos que suman < 1 (p. ej. $0.29),
            // flex solo repartiría esa fracción del ancho y la barra quedaría corta.
            style={{ flexGrow: (s.value / total) * 100, background: s.color }}
            onMouseMove={(e) =>
              setTip({
                x: e.clientX,
                y: e.clientY,
                content: (
                  <>
                    <b>{s.label}</b>
                    <div>
                      {format(s.value)} · {fmtPct(pct(s.value))}
                    </div>
                  </>
                ),
              })
            }
            onMouseLeave={() => setTip(null)}
          />
        ))}
      </div>
      <ul className="share-list">
        {items.map((s) => (
          <li key={s.key}>
            <i style={{ background: s.color }} />
            <span className="share-label" title={s.label}>
              {s.label}
            </span>
            <span className="share-pct num">{fmtPct(pct(s.value))}</span>
            {!compact && <span className="share-value num">{format(s.value)}</span>}
          </li>
        ))}
      </ul>
    </div>
  );
}

const fmtPct = (v: number) => `${Math.round(v * 100)}%`;

export const Legend = ({ items }: { items: { label: string; color: string }[] }) => (
  <div className="legend">
    {items.map((i) => (
      <span key={i.label}>
        <i style={{ background: i.color }} />
        {i.label}
      </span>
    ))}
  </div>
);
