/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** "1" en la build `perf` (`.env.perf`): instala el mock fuera de `DEV` para medir. */
  readonly VITE_MOCK?: string;
}
