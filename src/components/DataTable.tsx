import { memo, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Empty } from "./Panel";
import { t } from "../lib/i18n";
import { shouldVirtualize, visibleRange } from "../lib/virtual";

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

/** Filas ya ordenadas por lista, columna y sentido: se ordena una vez por dato, no en cada render. */
const sortedCache = new WeakMap<object, Map<(row: never) => number | string, { asc?: unknown[]; desc?: unknown[] }>>();

function sortedRows<T>(rows: T[], key: (row: T) => number | string, dir: "asc" | "desc"): T[] {
  let byKey = sortedCache.get(rows);
  if (!byKey) sortedCache.set(rows, (byKey = new Map()));
  let entry = byKey.get(key);
  if (!entry) byKey.set(key, (entry = {}));
  const hit = entry[dir];
  if (hit) return hit as T[];
  const sign = dir === "asc" ? 1 : -1;
  const out = [...rows].sort((a, b) => {
    const x = key(a);
    const y = key(b);
    return sign * (typeof x === "number" && typeof y === "number" ? x - y : String(x).localeCompare(String(y)));
  });
  entry[dir] = out;
  return out;
}

/** Alto de fila supuesto y alto de ventana antes de medir (primer render). */
const ROW_ESTIMATE = 32;
const VIEWPORT_ESTIMATE = 900;
const OVERSCAN = 8;

const scrollParent = (el: HTMLElement): HTMLElement | null => {
  for (let p = el.parentElement; p; p = p.parentElement) if (/(auto|scroll)/.test(getComputedStyle(p).overflowY)) return p;
  return null;
};

/** Rango de filas a pintar de una lista larga según el desplazamiento del contenedor más cercano. */
function useVirtualRows(enabled: boolean, count: number) {
  const top = useRef<HTMLDivElement>(null);
  const [rowHeight, setRowHeight] = useState(ROW_ESTIMATE);
  const [range, setRange] = useState(() => visibleRange({ scrollTop: 0, viewport: VIEWPORT_ESTIMATE, offsetTop: 0, rowHeight: ROW_ESTIMATE, count, overscan: OVERSCAN }));

  useLayoutEffect(() => {
    const first = top.current?.nextElementSibling;
    if (!enabled || !(first instanceof HTMLElement)) return;
    const measure = () => setRowHeight((h) => Math.round(first.getBoundingClientRect().height) || h);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(first);
    return () => ro.disconnect();
  }, [enabled]);

  useEffect(() => {
    const el = top.current;
    if (!enabled || !el) return;
    const box = scrollParent(el);
    const target: HTMLElement | Window = box ?? window;
    let frame = 0;
    const update = () => {
      frame = 0;
      const viewport = box ? box.clientHeight : window.innerHeight;
      const boxTop = box ? box.getBoundingClientRect().top : 0;
      const scrollTop = box ? box.scrollTop : window.scrollY;
      const offsetTop = el.getBoundingClientRect().top - boxTop + scrollTop;
      const next = visibleRange({ scrollTop, viewport, offsetTop, rowHeight, count, overscan: OVERSCAN });
      setRange((r) => (r.start === next.start && r.end === next.end ? r : next));
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    target.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll, { passive: true });
    return () => {
      if (frame) cancelAnimationFrame(frame);
      target.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
    };
  }, [enabled, rowHeight, count]);

  return { top, rowHeight, ...range };
}

interface RowProps<T> {
  row: T;
  columns: Column<T>[];
  template: string;
  onRowClick?: (row: T) => void;
  onRowHover?: (row: T | null, e?: React.MouseEvent) => void;
  rowClassName?: (row: T) => string;
  /** Solo en tablas virtualizadas. */
  rowIndex?: number;
}

const cellClass = <T,>(c: Column<T>, r: T) => (typeof c.className === "function" ? c.className(r) : (c.className ?? ""));

function TableRowView<T>({ row: r, columns, template, onRowClick, onRowHover, rowClassName, rowIndex }: RowProps<T>) {
  return (
    <div
      role="row"
      aria-rowindex={rowIndex}
      className={`table-row ${onRowClick ? "clickable" : ""} ${rowClassName?.(r) ?? ""}`}
      style={{ gridTemplateColumns: template }}
      tabIndex={onRowClick ? 0 : undefined}
      onClick={onRowClick ? () => onRowClick(r) : undefined}
      onKeyDown={onRowClick ? (e) => e.key === "Enter" && onRowClick(r) : undefined}
      onMouseMove={onRowHover ? (e) => onRowHover(r, e) : undefined}
      onMouseLeave={onRowHover ? () => onRowHover(null) : undefined}
    >
      {columns.map((c) => (
        <span key={c.header} role="cell" className={`${c.align === "right" ? "right num" : ""} ${cellClass(c, r)}`}>
          {c.cell(r)}
        </span>
      ))}
    </div>
  );
}

const TableRow = memo(TableRowView) as typeof TableRowView;

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

  const controlled = !!onSortChange;
  const sort = controlled ? controlledSort : localSort;
  const sortCol = !controlled && sort && columns.find((c) => c.header === sort.header && c.sort);
  if (sortCol) rows = sortedRows(rows, sortCol.sort!, sort!.dir);
  const toggleSort = (c: Column<T>) => (onSortChange ? onSortChange(nextSort(sort, c.header)) : setSort((cur) => nextSort(cur, c.header)));

  // Con `limit` manda la vista de resumen; sin él, se pliega a `collapse` filas (no por páginas).
  const selfCollapse = !more && limit == null && collapse !== false && rows.length > collapse;
  const shown = limit ? rows.slice(0, limit) : selfCollapse && !expanded ? rows.slice(0, collapse) : rows;
  const virtual = shouldVirtualize(shown.length);
  const win = useVirtualRows(virtual, shown.length);
  if (!rows.length) return <Empty>{empty}</Empty>;
  const painted = virtual ? shown.slice(win.start, win.end) : shown;

  const template = columns.map((c) => c.width ?? (c.align === "right" ? "72px" : "1fr")).join(" ");
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
    <div className="table" role="table" aria-rowcount={virtual ? shown.length : undefined} style={{ "--table-min": `${minWidth}px` } as React.CSSProperties}>
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
      {virtual && <div ref={win.top} aria-hidden="true" className="table-spacer" style={{ height: win.start * win.rowHeight }} />}
      {painted.map((r, i) => (
        <TableRow
          key={rowKey(r)}
          row={r}
          columns={columns}
          template={template}
          onRowClick={onRowClick}
          onRowHover={onRowHover}
          rowClassName={rowClassName}
          rowIndex={virtual ? win.start + i + 1 : undefined}
        />
      ))}
      {virtual && <div aria-hidden="true" className="table-spacer" style={{ height: (shown.length - win.end) * win.rowHeight }} />}
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
