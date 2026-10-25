/**
 * Caché de datos de la interfaz: la última respuesta de cada consulta por comando y filtro,
 * válida mientras no cambie la generación de los datos (avisos `data:changed`/`ingest://done`
 * y ajustes guardados). Al cambiar la generación solo se vuelven a pedir las consultas con
 * suscriptores (vistas montadas); las demás se piden al volver a suscribirse, mostrando antes
 * el dato anterior. Sin escucha del foco: recuperar la ventana no vuelve a pedir nada.
 */
import { useCallback, useRef, useSyncExternalStore } from "react";
import type { Filter } from "./api";

export interface QuerySnapshot<T = unknown> {
  data: T | undefined;
  error: string | null;
}

type Fetcher = () => Promise<unknown>;

interface Entry {
  snap: QuerySnapshot;
  /** Generación del dato guardado (−1 sin dato). */
  dataGeneration: number;
  fetchedAt: number;
  /** Generación de la petición en curso (−1 sin petición). */
  inflightGeneration: number;
  fetcher: Fetcher;
  listeners: Set<() => void>;
  lastUsed: number;
}

export interface QueryStore {
  snapshot(id: string): QuerySnapshot;
  /** Se suscribe a `id` y lo pide si no está fresco; devuelve la baja. */
  subscribe(id: string, fetcher: Fetcher, listener: () => void): () => void;
  generation(): number;
  /** Nueva generación de datos: siempre avanza, aunque `g` no sea mayor que la actual. */
  setGeneration(g: number): void;
  /** Ajustes guardados: todo lo cargado queda obsoleto. */
  invalidateAll(): void;
  onGeneration(listener: () => void): () => void;
}

const EMPTY: QuerySnapshot = { data: undefined, error: null };

export function createQueryStore({ now = Date.now, maxEntries = 64, staleMs = 600_000 }: { now?: () => number; maxEntries?: number; staleMs?: number } = {}): QueryStore {
  const entries = new Map<string, Entry>();
  const generationListeners = new Set<() => void>();
  let gen = 0;
  let tick = 0;

  const fresh = (e: Entry) => e.dataGeneration === gen && now() - e.fetchedAt < staleMs;

  const publish = (e: Entry, snap: QuerySnapshot) => {
    e.snap = snap;
    e.listeners.forEach((l) => l());
  };

  const load = (e: Entry) => {
    if (fresh(e) || e.inflightGeneration === gen) return;
    const g = gen;
    e.inflightGeneration = g;
    e.fetcher().then(
      (data) => {
        if (e.inflightGeneration === g) e.inflightGeneration = -1;
        if (g < e.dataGeneration) return;
        e.dataGeneration = g;
        e.fetchedAt = now();
        publish(e, { data, error: null });
      },
      (err) => {
        if (e.inflightGeneration === g) e.inflightGeneration = -1;
        if (g < e.dataGeneration) return;
        publish(e, { data: e.snap.data, error: String(err) });
      },
    );
  };

  const evict = () => {
    if (entries.size <= maxEntries) return;
    const idle = [...entries].filter(([, e]) => !e.listeners.size).sort(([, a], [, b]) => a.lastUsed - b.lastUsed);
    for (const [id] of idle) {
      if (entries.size <= maxEntries) break;
      entries.delete(id);
    }
  };

  const bump = () => {
    for (const e of entries.values()) if (e.listeners.size) load(e);
    generationListeners.forEach((l) => l());
  };

  return {
    snapshot: (id) => entries.get(id)?.snap ?? EMPTY,
    subscribe(id, fetcher, listener) {
      let e = entries.get(id);
      if (!e) {
        e = { snap: EMPTY, dataGeneration: -1, fetchedAt: 0, inflightGeneration: -1, fetcher, listeners: new Set(), lastUsed: 0 };
        entries.set(id, e);
      }
      const entry = e;
      entry.fetcher = fetcher;
      entry.lastUsed = ++tick;
      entry.listeners.add(listener);
      load(entry);
      evict();
      return () => {
        entry.listeners.delete(listener);
        entry.lastUsed = ++tick;
        evict();
      };
    },
    generation: () => gen,
    setGeneration(g) {
      gen = Math.max(g, gen + 1);
      bump();
    },
    invalidateAll() {
      gen += 1;
      bump();
    },
    onGeneration(listener) {
      generationListeners.add(listener);
      return () => generationListeners.delete(listener);
    },
  };
}

/** Clave de un filtro: campos en orden fijo; sin filtro de agentes (`*`) no es lo mismo que ninguno (`[]`). */
export function filterKey(f: Filter): string {
  const list = (xs?: (string | number)[]) => (xs ? `[${xs.join(",")}]` : "*");
  return `${f.from ?? ""}|${f.to ?? ""}|${list(f.agents)}|${list(f.projects)}`;
}

/** La caché de la ventana principal. */
export const queryStore = createQueryStore();

/** Datos de `cmd` para `key` (p. ej. `filterKey(filter)`), pedidos con `fetcher` si no están frescos. */
export function useQuery<T>(cmd: string, key: string, fetcher: () => Promise<T>, store: QueryStore = queryStore): QuerySnapshot<T> {
  const id = `${cmd}|${key}`;
  const latest = useRef(fetcher);
  latest.current = fetcher;
  const subscribe = useCallback((cb: () => void) => store.subscribe(id, () => latest.current(), cb), [store, id]);
  const read = () => store.snapshot(id) as QuerySnapshot<T>;
  return useSyncExternalStore(subscribe, read, read);
}

/** Generación de datos actual; cambia con cada aviso de datos y al guardar los ajustes. */
export function useGeneration(store: QueryStore = queryStore): number {
  return useSyncExternalStore(store.onGeneration, store.generation, store.generation);
}
