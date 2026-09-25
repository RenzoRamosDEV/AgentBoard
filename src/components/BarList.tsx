import type { ReactNode } from "react";
import { useTooltip } from "./Tooltip";
import { Empty } from "./Panel";

export interface BarItem {
  key: string;
  label: string;
  value: number;
  valueLabel: string;
  badge?: string;
  tooltip: ReactNode;
}

/** Barras horizontales de una sola serie, ordenadas por valor. */
export function BarList({ items, max = 10 }: { items: BarItem[]; max?: number }) {
  const setTip = useTooltip();
  if (!items.length) return <Empty />;
  const shown = items.slice(0, max);
  const top = Math.max(...shown.map((i) => i.value), 1e-12);
  return (
    <ul className="barlist">
      {shown.map((i) => (
        <li
          key={i.key}
          onMouseMove={(e) => setTip({ x: e.clientX, y: e.clientY, content: i.tooltip })}
          onMouseLeave={() => setTip(null)}
        >
          <div className="barlist-text">
            <span className="barlist-label" title={i.label}>
              {i.label}
              {i.badge && <span className="badge">{i.badge}</span>}
            </span>
            <span className="barlist-value">{i.valueLabel}</span>
          </div>
          <div className="barlist-track">
            <div className="barlist-bar" style={{ width: `${Math.max((i.value / top) * 100, i.value > 0 ? 1 : 0)}%` }} />
          </div>
        </li>
      ))}
      {items.length > max && <li className="muted barlist-more">y {items.length - max} más</li>}
    </ul>
  );
}
