import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Empty } from "./Panel";
import { useTooltip } from "./Tooltip";

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
          style={{ gridTemplateColumns: `${labelWidth}px 1fr auto` }}
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

/** Gráfico de columnas por día (o apilado por serie), con eje de fechas y tooltip. */
export function Columns({
  points,
  height = 190,
  color = "var(--accent)",
  format,
  axis = (ts: number) => new Date(ts).toLocaleDateString("es-ES", { day: "numeric", month: "short" }),
}: {
  points: ColumnPoint[];
  height?: number;
  color?: string;
  format: (v: number) => string;
  axis?: (ts: number) => string;
}) {
  const setTip = useTooltip();
  const [ref] = useWidth<HTMLDivElement>();
  if (!points.length) return <Empty />;
  const max = Math.max(...points.map((p) => p.value), 1e-12);
  const px = (v: number) => Math.round((v / max) * height);
  const n = points.length;
  const mid = Math.floor(n / 2);
  const template = { gridTemplateColumns: `repeat(${n}, minmax(0, 1fr))` };
  return (
    <div className="columns" ref={ref}>
      <div className="columns-max muted">máx. {format(max)}</div>
      <div className="columns-plot" style={{ height, ...template }}>
        {points.map((p) => (
          <div
            key={p.ts}
            className="column"
            style={{ height }}
            onMouseMove={p.tooltip ? (e) => setTip({ x: e.clientX, y: e.clientY, content: p.tooltip }) : undefined}
            onMouseLeave={p.tooltip ? () => setTip(null) : undefined}
          >
            {p.stack
              ? [...p.stack].reverse().map((s) => <div key={s.key} className="column-seg" style={{ height: px(s.value), background: s.color }} />)
              : <div className="column-seg column-single" style={{ height: px(p.value), background: color }} />}
          </div>
        ))}
      </div>
      <div className="columns-axis muted" style={template}>
        {points.map((p, i) => (
          <span key={p.ts} className={i === 0 ? "first" : i === n - 1 ? "last" : ""}>
            {i === 0 || i === n - 1 || (n > 6 && i === mid) ? axis(p.ts) : ""}
          </span>
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
