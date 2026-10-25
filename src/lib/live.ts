/** Avisos del backend que refrescan las vistas. */
import type { DataChanged } from "./api";
import type { QueryStore } from "./query";

/** Cambiaron datos confirmados (como mucho uno por segundo). */
export const DATA_CHANGED = "data:changed";
/** Terminó el escaneo inicial o el de una reconstrucción de la caché. */
export const INGEST_DONE = "ingest://done";
/** Se abrió el panel de la bandeja. */
export const PANEL_SHOWN = "panel://shown";

/** La forma de `listen` de Tauri que usan las suscripciones (inyectable en los tests). */
export type Listen = <T>(event: string, cb: (e: { payload: T }) => void) => Promise<() => void>;

const releaseAll = (offs: Promise<() => void>[]) => () => {
  for (const off of offs) void off.then((f) => f());
};

/**
 * Ventana principal: cada aviso de datos cambia la generación de la caché de consultas (que
 * vuelve a pedir solo lo montado); el indicador en vivo se resalta solo con los avisos que llegan
 * tras el primer `ingest://done` (los del escaneo inicial no son «en vivo»).
 */
export function subscribeApp(
  listen: Listen,
  store: Pick<QueryStore, "generation" | "setGeneration">,
  h: { changed: () => void; live: () => void },
): () => void {
  let initialDone = false;
  return releaseAll([
    listen(INGEST_DONE, () => {
      initialDone = true;
      store.setGeneration(store.generation() + 1);
      h.changed();
    }),
    listen<DataChanged>(DATA_CHANGED, (e) => {
      store.setGeneration(e.payload.generation);
      h.changed();
      if (initialDone) h.live();
    }),
  ]);
}

/** Panel de la bandeja: refresca con cada aviso de datos y al abrirse; sin intervalos. */
export function subscribeTray(listen: Listen, bump: () => void): () => void {
  return releaseAll([listen<DataChanged>(DATA_CHANGED, bump), listen(PANEL_SHOWN, bump)]);
}

/** Ejecuta `fn` solo con la ventana visible: oculta no hay nadie mirando. */
export function whenVisible(doc: { visibilityState: string }, fn: () => void) {
  if (doc.visibilityState !== "hidden") fn();
}
