import { useState, type ReactNode } from "react";
import { Empty } from "./Panel";
import { t } from "../lib/i18n";

export interface Column<T> {
  header: string;
  cell: (row: T) => ReactNode;
  /** Ancho de la columna en la plantilla de la rejilla (`1fr`, `72px`). */
  width?: string;
  align?: "left" | "right";
  /** Clase para colorear (`cost`, `accent`, `muted`). */
  className?: string | ((row: T) => string);
}

/** Al mostrar la lista completa, se pliega a estas filas con un botón para desplegar. */
const COLLAPSE = 20;

/** Tabla compacta en rejilla: cabecera discreta y filas con líneas finas. */
export function DataTable<T>({
  rows,
  rowKey,
  columns,
  limit,
  empty,
  onRowHover,
  onMore,
  collapse = COLLAPSE,
}: {
  rows: T[];
  rowKey: (row: T) => string;
  columns: Column<T>[];
  limit?: number;
  empty?: string;
  onRowHover?: (row: T | null, e?: React.MouseEvent) => void;
  /** Con `limit`, enlace "Ver más" que abre la vista ampliada. */
  onMore?: () => void;
  /** Sin `limit`, cuántas filas mostrar plegado (`false` para no plegar). */
  collapse?: number | false;
}) {
  const [expanded, setExpanded] = useState(false);
  if (!rows.length) return <Empty>{empty}</Empty>;

  // Con `limit` manda la vista de resumen; sin él, se pliega a `collapse` filas.
  const selfCollapse = limit == null && collapse !== false && rows.length > collapse;
  const shown = limit ? rows.slice(0, limit) : selfCollapse && !expanded ? rows.slice(0, collapse) : rows;

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
              {t("Ver más ›")}
            </button>
          ) : (
            <span className="muted">{t("… y {n} más", { n: rows.length - limit })}</span>
          )}
        </div>
      )}
      {selfCollapse && (
        <div className="table-more">
          <button
            className="link"
            aria-expanded={expanded}
            onClick={() => setExpanded((v) => !v)}
          >
            {expanded ? t("Ver menos ▴") : t("Ver todo ({n}) ▾", { n: rows.length })}
          </button>
        </div>
      )}
    </div>
  );
}
