/**
 * Cifras del arnés de medición de la interfaz. Sin dependencias: lo usan el arnés en el
 * navegador y `scripts/perf-ui.mjs` en Node.
 */

export interface Stat {
  n: number;
  median: number | null;
  max: number | null;
}

export function median(xs: number[]): number | null {
  if (!xs.length) return null;
  const s = [...xs].sort((a, b) => a - b);
  const mid = s.length >> 1;
  return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
}

const tenth = (x: number) => Math.round(x * 10) / 10;

export function summarize(xs: number[]): Stat {
  const m = median(xs);
  return { n: xs.length, median: m == null ? null : tenth(m), max: xs.length ? tenth(Math.max(...xs)) : null };
}

const ENTITIES: Record<string, string> = { "&amp;": "&", "&lt;": "<", "&gt;": ">", "&quot;": '"', "&#39;": "'" };

/** El JSON que el arnés deja en `<pre id="perf-results">`, sacado del DOM volcado por Chromium. */
export function parseResults(html: string): unknown {
  const m = /<pre id="perf-results"[^>]*>([\s\S]*?)<\/pre>/.exec(html);
  if (!m) throw new Error("sin <pre id=\"perf-results\"> en el DOM: el arnés no terminó");
  return JSON.parse(m[1].replace(/&(amp|lt|gt|quot|#39);/g, (e) => ENTITIES[e]));
}
