import type { CSSProperties, ReactNode } from "react";
import { useTooltip } from "./Tooltip";

/** Panel con borde de color al estilo terminal. */
export function TermPanel({
  title,
  color,
  className = "",
  children,
}: {
  title: ReactNode;
  color: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section className={`term-panel ${className}`} style={{ "--panel": color } as CSSProperties}>
      <h2>{title}</h2>
      {children}
    </section>
  );
}

/** Barra de calor: el tramo relleno muestra el degradado azul → rojo hasta su valor. */
export function HeatBar({ value, max }: { value: number; max: number }) {
  const pct = max > 0 ? Math.max((value / max) * 100, value > 0 ? 4 : 0) : 0;
  return (
    <span className="heatbar" aria-hidden>
      <span className="heatbar-fill" style={{ width: `${pct}%` }} />
    </span>
  );
}

export interface Column<T> {
  header: string;
  cell: (row: T) => ReactNode;
  className?: string;
}

/** Tabla compacta: barra de calor, etiqueta y columnas numéricas alineadas a la derecha. */
export function TermTable<T>({
  rows,
  rowKey,
  value,
  label,
  columns,
  tooltip,
  limit = 12,
  empty = "sin datos",
}: {
  rows: T[];
  rowKey: (row: T) => string;
  value: (row: T) => number;
  label: (row: T) => ReactNode;
  columns: Column<T>[];
  tooltip?: (row: T) => ReactNode;
  limit?: number;
  empty?: string;
}) {
  const setTip = useTooltip();
  if (!rows.length) return <p className="term-empty">{empty}</p>;
  const shown = rows.slice(0, limit);
  const max = Math.max(...shown.map(value));
  return (
    <table className="term-table">
      <thead>
        <tr>
          <th className="bar-col" />
          <th />
          {columns.map((c) => (
            <th key={c.header} className="num">
              {c.header}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {shown.map((r) => (
          <tr
            key={rowKey(r)}
            onMouseMove={tooltip ? (e) => setTip({ x: e.clientX, y: e.clientY, content: tooltip(r) }) : undefined}
            onMouseLeave={tooltip ? () => setTip(null) : undefined}
          >
            <td className="bar-col">
              <HeatBar value={value(r)} max={max} />
            </td>
            <td className="label">{label(r)}</td>
            {columns.map((c) => (
              <td key={c.header} className={`num ${c.className ?? ""}`}>
                {c.cell(r)}
              </td>
            ))}
          </tr>
        ))}
        {rows.length > limit && (
          <tr>
            <td />
            <td className="muted" colSpan={columns.length + 1}>
              … y {rows.length - limit} más
            </td>
          </tr>
        )}
      </tbody>
    </table>
  );
}

export const Cost = ({ v }: { v: number }) => <span className="cost">{v === 0 ? "$0.00" : `$${v < 1 ? v.toFixed(3) : v.toFixed(2)}`}</span>;

export const OneShot = ({ v }: { v: number | null | undefined }) =>
  v == null ? <span className="muted">-</span> : <span className={v >= 0.995 ? "good" : "warn"}>{(v * 100).toFixed(v >= 0.995 ? 0 : 1)}%</span>;
