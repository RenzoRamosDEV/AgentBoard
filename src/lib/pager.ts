/** Listas que el servidor entrega por páginas: la primera al cargar y las siguientes con «más». */
import type { Page } from "./api";

export interface PagerState<T> {
  rows: T[];
  total: number;
}

/**
 * `load(sort)` pide la primera página (con otro orden o tras un aviso de datos cambiados) y
 * conserva las filas actuales hasta que llega; `more()` añade la siguiente. Una respuesta de una
 * carga anterior se descarta.
 */
export function createPager<T, S>(
  fetch: (offset: number, limit: number, sort: S) => Promise<Page<T>>,
  pageSize: number,
  onChange: (state: PagerState<T>) => void,
) {
  let state: PagerState<T> = { rows: [], total: 0 };
  let sort: S | undefined;
  let token = 0;
  let busy = false;
  const set = (s: PagerState<T>) => {
    state = s;
    onChange(s);
  };
  const request = (offset: number, s: S, apply: (p: Page<T>) => PagerState<T>) => {
    const mine = ++token;
    busy = true;
    fetch(offset, pageSize, s)
      .then((p) => {
        if (mine !== token) return;
        busy = false;
        set(apply(p));
      })
      .catch(() => {
        if (mine === token) busy = false;
      });
  };
  return {
    state: () => state,
    load(next: S) {
      sort = next;
      request(0, next, (p) => ({ rows: p.rows, total: p.total }));
    },
    more() {
      if (sort === undefined || busy || state.rows.length >= state.total) return;
      const offset = state.rows.length;
      request(offset, sort, (p) => ({ rows: [...state.rows, ...p.rows], total: p.total }));
    },
  };
}
