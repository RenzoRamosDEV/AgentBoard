/** Agrupaciones de los paneles, puras: las vistas las calculan una vez por dato y métrica. */

/** Filas ordenadas por `get` de mayor a menor: como mucho `limit`; si hay más, la última plaza agrupa el resto. */
export function rankItems<T>(rows: T[], get: (r: T) => number, isRate: boolean, limit: number) {
  const sorted = rows.map((r, i) => ({ r, i })).sort((a, b) => get(b.r) - get(a.r));
  const head = sorted.length > limit ? sorted.slice(0, limit - 1) : sorted;
  const tail = sorted.slice(head.length);
  // Los porcentajes y medias no se suman: para «Otros» se usa la media.
  const values = tail.map(({ r }) => get(r));
  const sum = values.reduce((a, v) => a + v, 0);
  return { head, tail, others: tail.length ? (isRate ? sum / values.length : sum) : null };
}

/** Total de cada clave de una serie, de mayor a menor, con la etiqueta de su primera aparición. */
export function rankKeys<S extends { key: string }>(series: S[], get: (s: S) => number, label: (s: S) => string) {
  const totals = new Map<string, { key: string; label: string; value: number }>();
  for (const s of series) {
    const cur = totals.get(s.key) ?? { key: s.key, label: label(s), value: 0 };
    cur.value += get(s);
    totals.set(s.key, cur);
  }
  return [...totals.values()].sort((a, b) => b.value - a.value);
}

export interface StackItem {
  key: string;
  label: string;
  value: number;
  color: string;
}

/** Suma por cubo y clave; cada pila en orden de aparición, con la etiqueta y el color de la primera. */
export function stacksByBucket<S>(
  items: S[],
  bucket: (s: S) => number,
  key: (s: S) => string,
  label: (s: S) => string,
  value: (s: S) => number,
  color: (s: S) => string,
): Map<number, StackItem[]> {
  const out = new Map<number, StackItem[]>();
  const index = new Map<number, Map<string, StackItem>>();
  for (const s of items) {
    const b = bucket(s);
    const k = key(s);
    let list = out.get(b);
    let byKey = index.get(b);
    if (!list || !byKey) {
      out.set(b, (list = []));
      index.set(b, (byKey = new Map()));
    }
    const cur = byKey.get(k);
    if (cur) cur.value += value(s);
    else {
      const item = { key: k, label: label(s), value: value(s), color: color(s) };
      byKey.set(k, item);
      list.push(item);
    }
  }
  return out;
}
