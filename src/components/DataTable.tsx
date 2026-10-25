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
  /** Valor por el que ordenar al pulsar la cabecera; sin él, la columna no se ordena. */
  sort?: (row: T) => number | string;
}

export interface SortState {
  /** Cabecera de la columna por la que se ordena. */
  header: string;
  dir: "asc" | "desc";
}

/** Siguiente orden al pulsar una cabecera: primero descendente; otra vez, ascendente. */
export const nextSort = (cur: SortState | undefined, header: string): SortState => ({
  header,
  dir: cur?.header === header && cur.dir === "desc" ? "asc" : "desc",
});

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
  onRowClick,
  defaultSort,
  rowClassName,
  sort: controlledSort,
  onSortChange,
  more,
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
  /** Hace la fila pulsable (clic o Enter). */
  onRowClick?: (row: T) => void;
  defaultSort?: SortState;
  rowClassName?: (row: T) => string;
  /** Orden controlado desde fuera (el servidor ordena): la tabla no reordena las filas. */
  sort?: SortState;
  onSortChange?: (sort: SortState) => void;
  /** Lista por páginas: total en el servidor y cómo pedir la siguiente. */
  more?: { total: number; onMore: () => void };
}) {
  const [expanded, setExpanded] = useState(false);
  const [localSort, setSort] = useState<SortState | undefined>(defaultSort);
  if (!rows.length) return <Empty>{empty}</Empty>;

  const controlled = !!onSortChange;
  const sort = controlled ? controlledSort : localSort;
  const sortCol = !controlled && sort && columns.find((c) => c.header === sort.header && c.sort);
  if (sortCol) {
    const key = sortCol.sort!;
    const sign = sort!.dir === "asc" ? 1 : -1;
    rows = [...rows].sort((a, b) => {
      const x = key(a);
      const y = key(b);
      return sign * (typeof x === "number" && typeof y === "number" ? x - y : String(x).localeCompare(String(y)));
    });
  }
  const toggleSort = (c: Column<T>) => (onSortChange ? onSortChange(nextSort(sort, c.header)) : setSort((cur) => nextSort(cur, c.header)));

  // Con `limit` manda la vista de resumen; sin él, se pliega a `collapse` filas (no por páginas).
  const selfCollapse = !more && limit == null && collapse !== false && rows.length > collapse;
  const shown = limit ? rows.slice(0, limit) : selfCollapse && !expanded ? rows.slice(0, collapse) : rows;

  const template = columns.map((c) => c.width ?? (c.align === "right" ? "72px" : "1fr")).join(" ");
  const cls = (c: Column<T>, r: T) => (typeof c.className === "function" ? c.className(r) : (c.className ?? ""));
  // Suelo de ancho por columna: por debajo, la tabla hace scroll horizontal en vez de
  // recortar datos (mínimo declarado de cada columna; ~96 px para las flexibles).
  const minOf = (c: Column<T>) => {
    const w = c.width ?? "";
    const mm = w.match(/^minmax\((\d+(?:\.\d+)?)px/);
    if (mm) return Number(mm[1]);
    const px = w.match(/^(\d+(?:\.\d+)?)px$/);
    if (px) return Number(px[1]);
    return c.align === "right" ? 72 : 96;
  };
  const minWidth = columns.reduce((a, c) => a + minOf(c), 0) + 8 * (columns.length - 1) + 16;
  return (
    <div className="table" role="table" style={{ "--table-min": `${minWidth}px` } as React.CSSProperties}>
      <div className="table-head" role="row" style={{ gridTemplateColumns: template }}>
        {columns.map((c) => (
          <span
            key={c.header}
            role="columnheader"
            className={c.align === "right" ? "right" : ""}
            aria-sort={sort?.header === c.header ? (sort.dir === "asc" ? "ascending" : "descending") : undefined}
          >
            {c.sort ? (
              <button type="button" className={`sort-button ${sort?.header === c.header ? "active" : ""}`} onClick={() => toggleSort(c)}>
                {c.header}
                {sort?.header === c.header && <span aria-hidden>{sort.dir === "asc" ? " ↑" : " ↓"}</span>}
              </button>
            ) : (
              c.header
            )}
          </span>
        ))}
      </div>
      {shown.map((r) => (
        <div
          key={rowKey(r)}
          role="row"
          className={`table-row ${onRowClick ? "clickable" : ""} ${rowClassName?.(r) ?? ""}`}
          style={{ gridTemplateColumns: template }}
          tabIndex={onRowClick ? 0 : undefined}
          onClick={onRowClick ? () => onRowClick(r) : undefined}
          onKeyDown={onRowClick ? (e) => e.key === "Enter" && onRowClick(r) : undefined}
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
      {more && rows.length < more.total && (
        <div className="table-more">
          <button className="link" onClick={more.onMore}>
            {t("Cargar más ({n} de {total})", { n: rows.length, total: more.total })}
          </button>
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
