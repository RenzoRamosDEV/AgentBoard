/** Filas de una tabla larga que hay que pintar: las visibles en el contenedor desplazable más un margen. */

/** Por encima de este número de filas mostradas se virtualiza; con menos se pintan todas. */
export const VIRTUALIZE_ABOVE = 200;

export const shouldVirtualize = (count: number) => count > VIRTUALIZE_ABOVE;

export interface RangeInput {
  /** Desplazamiento del contenedor. */
  scrollTop: number;
  /** Alto visible del contenedor. */
  viewport: number;
  /** Posición de la primera fila dentro del contenido del contenedor. */
  offsetTop: number;
  rowHeight: number;
  count: number;
  /** Filas de margen por arriba y por abajo. */
  overscan: number;
}

/** Filas `[start, end)` a pintar. */
export function visibleRange({ scrollTop, viewport, offsetTop, rowHeight, count, overscan }: RangeInput): { start: number; end: number } {
  if (rowHeight <= 0 || count <= 0) return { start: 0, end: 0 };
  const clamp = (n: number) => Math.min(count, Math.max(0, n));
  const first = Math.floor((scrollTop - offsetTop) / rowHeight);
  const last = Math.ceil((scrollTop + viewport - offsetTop) / rowHeight);
  return { start: clamp(first - overscan), end: clamp(last + overscan) };
}
