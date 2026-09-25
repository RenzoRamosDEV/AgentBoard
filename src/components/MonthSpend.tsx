import { useMemo, useRef, useState, useLayoutEffect } from "react";
import type { Point } from "../lib/api";
import { fmt } from "../lib/format";
import { daysInMonth, monthStart, projectMonth } from "../lib/period";
import { useTooltip } from "./Tooltip";

const H = 220;
const M = { top: 16, right: 56, bottom: 26, left: 52 };

function useWidth() {
  const ref = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(600);
  useLayoutEffect(() => {
    if (!ref.current) return;
    const ro = new ResizeObserver(([e]) => setW(e.contentRect.width));
    ro.observe(ref.current);
    return () => ro.disconnect();
  }, []);
  return [ref, w] as const;
}

/** Acumulado del mes en curso, proyección lineal y línea de presupuesto. */
export function MonthSpend({ points, budget }: { points: Point[]; budget: number | null }) {
  const [ref, width] = useWidth();
  const setTip = useTooltip();
  const now = new Date();
  const days = daysInMonth(now);
  const today = now.getDate();

  const { cumulative, daily, spent, projection } = useMemo(() => {
    const daily = new Array(days + 1).fill(0);
    const start = monthStart(now).getTime();
    for (const p of points) {
      const d = new Date(p.ts).getDate();
      if (p.ts >= start && d >= 1 && d <= days) daily[d] += p.costUsd;
    }
    const cumulative: number[] = [0];
    for (let d = 1; d <= today; d++) cumulative[d] = cumulative[d - 1] + daily[d];
    const spent = cumulative[today];
    return { cumulative, daily, spent, projection: projectMonth(spent, now) };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [points, days, today]);

  const yMax = Math.max(projection, budget ?? 0, spent, 0.01) * 1.1;
  const iw = Math.max(width - M.left - M.right, 50);
  const ih = H - M.top - M.bottom;
  const x = (d: number) => M.left + ((d - 1) / (days - 1)) * iw;
  const y = (v: number) => M.top + ih - (v / yMax) * ih;
  const ticks = [0, yMax / 2 / 1.1, yMax / 1.1].map((v) => Math.round(v * 100) / 100);

  const line = cumulative
    .slice(1)
    .map((v, i) => `${i ? "L" : "M"}${x(i + 1).toFixed(1)},${y(v).toFixed(1)}`)
    .join("");
  const [hover, setHover] = useState<number | null>(null);
  const over = budget != null && projection > budget;

  const onMove = (e: React.MouseEvent<SVGRectElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const d = Math.min(today, Math.max(1, Math.round(((e.clientX - rect.left) / rect.width) * (days - 1)) + 1));
    setHover(d);
    const date = new Date(now.getFullYear(), now.getMonth(), d);
    setTip({
      x: e.clientX,
      y: e.clientY,
      content: (
        <>
          <b>{fmt.day(date.getTime())}</b>
          <div>Acumulado: {fmt.usd(cumulative[d])}</div>
          <div className="muted">Ese día: {fmt.usd(daily[d])}</div>
        </>
      ),
    });
  };

  return (
    <div ref={ref}>
      <div className="month-stats">
        <div>
          <span className="muted">Llevas</span>
          <strong>{fmt.usd(spent)}</strong>
        </div>
        <div>
          <span className="muted">Proyección</span>
          <strong>{fmt.usd(projection)}</strong>
        </div>
        {budget != null && (
          <div>
            <span className="muted">Presupuesto</span>
            <strong>
              {fmt.usd(budget)} <small className="muted">({fmt.pct(budget ? spent / budget : 0)} usado)</small>
            </strong>
          </div>
        )}
        {over && <span className="status status-warning">⚠ La proyección supera el presupuesto</span>}
      </div>
      <svg width={width} height={H} role="img" aria-label={`Gasto acumulado del mes: ${fmt.usd(spent)}, proyección ${fmt.usd(projection)}`}>
        {ticks.map((t) => (
          <g key={t}>
            <line className="grid" x1={M.left} x2={M.left + iw} y1={y(t)} y2={y(t)} />
            <text className="axis" x={M.left - 8} y={y(t) + 4} textAnchor="end">
              {fmt.usd(t)}
            </text>
          </g>
        ))}
        {[1, Math.ceil(days / 2), days].map((d) => (
          <text key={d} className="axis" x={x(d)} y={H - 6} textAnchor="middle">
            {d}
          </text>
        ))}
        {budget != null && (
          <g>
            <line className="ref-line" x1={M.left} x2={M.left + iw} y1={y(budget)} y2={y(budget)} />
            <text className="axis" x={M.left + iw + 6} y={y(budget) + 4}>
              límite
            </text>
          </g>
        )}
        <line className="projection" x1={x(today)} y1={y(spent)} x2={x(days)} y2={y(projection)} />
        <path className="series-line" d={line} />
        <circle className="series-dot" cx={x(today)} cy={y(spent)} r={4} />
        <text className="axis" x={x(days) + 6} y={y(projection) + 4}>
          {fmt.usd(projection)}
        </text>
        {hover && (
          <>
            <line className="crosshair" x1={x(hover)} x2={x(hover)} y1={M.top} y2={M.top + ih} />
            <circle className="series-dot" cx={x(hover)} cy={y(cumulative[hover])} r={4} />
          </>
        )}
        <rect
          x={M.left}
          y={M.top}
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
    </div>
  );
}
