/** Ajustes de la caché de datos: «Solo memoria» y la acción «Reconstruir caché». */
import type { Settings } from "./api";

export type RebuildState = "idle" | "running";

/**
 * Reconstrucción de la caché: en curso desde que se pide hasta que el backend avisa con
 * `ingest://done` (fin de la relectura de todos los logs). El oyente se registra antes de
 * pedirla para no perder el aviso.
 */
export function createRebuild(
  rebuild: () => Promise<void>,
  listenDone: (cb: () => void) => (() => void) | Promise<() => void>,
  onChange: (state: RebuildState) => void,
) {
  let state: RebuildState = "idle";
  let error: string | null = null;
  let off: (() => void) | null = null;
  const current = () => state;
  const set = (s: RebuildState) => {
    state = s;
    onChange(s);
  };
  const finish = () => {
    off?.();
    off = null;
    if (state === "running") set("idle");
  };
  return {
    state: current,
    error: () => error,
    async start() {
      if (state === "running") return;
      error = null;
      set("running");
      off = await listenDone(finish);
      // `finish` pudo llegar mientras se registraba el oyente.
      if (current() !== "running") {
        off();
        off = null;
        return;
      }
      try {
        await rebuild();
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
        finish();
      }
    },
    dispose() {
      off?.();
      off = null;
    },
  };
}

/** ¿Hay cambios en el borrador del diálogo de ajustes que aplicar? */
export const isDirty = (draft: Settings, saved: Settings) =>
  draft.theme !== saved.theme || draft.language !== saved.language || draft.memoryOnly !== saved.memoryOnly;
