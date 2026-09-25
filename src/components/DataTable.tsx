import type { ReactNode } from "react";
import { Empty } from "./Panel";

export interface Column<T> {
  header: string;
  cell: (row: T) => ReactNode;
  /** Ancho de la columna en la plantilla de la rejilla (`1fr`, `72px`). */
  width?: string;
  align?: "left" | "right";
  /** Clase para colorear (`cost`, `accent`, `muted`). */
  className?: string | ((row: T) => string);
}

/** Tabla compacta en rejilla: cabecera discreta y filas con líneas finas. */
export function DataTable<T>({
  rows,
  rowKey,
  columns,
  limit,
  empty,
  onRowHover,
  onMore,
}: {
  rows: T[];
  rowKey: (row: T) => string;
  columns: Column<T>[];
  limit?: number;
  empty?: string;
  onRowHover?: (row: T | null, e?: React.MouseEvent) => void;
  /** Con `limit`, enlace "Ver más" que abre la vista ampliada. */
  onMore?: () => void;
}) {
  if (!rows.length) return <Empty>{empty}</Empty>;
  const shown = limit ? rows.slice(0, limit) : rows;
  const template = columns.map((c) => c.width ?? (c.align === "right" ? "72px" : "1fr")).join(" ");
  const cls = (c: Column<T>, r: T) => (typeof c.className === "function" ? c.className(r) : (c.className ?? ""));
  return (
    <div className="table" role="table">
      <div className="table-head" role="row" style={{ gridTemplateColumns: template }}>
        {columns.map((c) => (
          <span key={c.header} role="columnheader" className={c.align === "right" ? "right" : ""}>
            {c.header}
          </span>
        ))}
      </div>
      {shown.map((r) => (
        <div
          key={rowKey(r)}
          role="row"
          className="table-row"
          style={{ gridTemplateColumns: template }}
          onMouseMove={onRowHover ? (e) => onRowHover(r, e) : undefined}
          onMouseLeave={onRowHover ? () => onRowHover(null) : undefined}
        >
          {columns.map((c) => (
            <span key={c.header} role="cell" className={`${c.align === "right" ? "right num" : ""} ${cls(c, r)}`}>
              {c.cell(r)}
            </span>
          ))}
        </div>
      ))}
      {limit && rows.length > limit && (
        <div className="table-more">
          {onMore ? (
            <button className="link" onClick={onMore}>
              Ver {rows.length - limit} más ›
            </button>
          ) : (
            <span className="muted">… y {rows.length - limit} más</span>
          )}
        </div>
      )}
    </div>
  );
}
