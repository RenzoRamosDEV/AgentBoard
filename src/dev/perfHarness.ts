/**
 * Arnés de medición de la interfaz (solo en la build `perf`, con `?perf=1`): ejecuta los
 * escenarios S1–S4 con clics reales sobre la app montada con el mock y deja el JSON de
 * resultados en `<pre id="perf-results">`, que lee `scripts/perf-ui.mjs`.
 */
import { Profiler, createElement, type ReactNode } from "react";
import { emit } from "@tauri-apps/api/event";
import { summarize } from "./perfStats";

const REPS = 5;
/** Sin peticiones ni cambios en el contenido durante este tiempo: la interacción terminó. */
const QUIET_MS = 200;
const TIMEOUT_MS = 15_000;

interface Probe {
  pending: number;
  requests: Map<string, number>;
  total: number;
  lastActivity: number;
  responseSinceCommit: boolean;
  lastPaint: number;
  renderMs: number;
}

const probe: Probe = { pending: 0, requests: new Map(), total: 0, lastActivity: 0, responseSinceCommit: false, lastPaint: 0, renderMs: 0 };

const now = () => performance.now();
const frame = () => new Promise<void>((r) => requestAnimationFrame(() => r()));
/** Hora del fotograma pintado tras un cambio: el del segundo `requestAnimationFrame`. */
const markPaint = () => requestAnimationFrame(() => requestAnimationFrame(() => (probe.lastPaint = now())));

type Internals = { invoke: (cmd: string, args?: unknown, opts?: unknown) => Promise<unknown> };

function countRequests() {
  const internals = (window as unknown as { __TAURI_INTERNALS__: Internals }).__TAURI_INTERNALS__;
  const invoke = internals.invoke;
  internals.invoke = (cmd, args, opts) => {
    // Los comandos `plugin:event|…` son la escucha de avisos, no peticiones de datos.
    if (cmd.startsWith("plugin:")) return invoke(cmd, args, opts);
    probe.pending++;
    probe.total++;
    probe.requests.set(cmd, (probe.requests.get(cmd) ?? 0) + 1);
    probe.lastActivity = now();
    return invoke(cmd, args, opts).finally(() => {
      probe.pending--;
      probe.lastActivity = now();
      probe.responseSinceCommit = true;
    });
  };
}

/** Cambios en el DOM fuera de la barra superior y del panel lateral (su reloj y sus botones no cuentan). */
function watchContent() {
  const outside = (n: Node) => {
    const el = n instanceof Element ? n : n.parentElement;
    return !!el?.closest(".topbar, .sidebar, #perf-results");
  };
  new MutationObserver((records) => {
    if (records.every((r) => outside(r.target))) return;
    probe.lastActivity = now();
    markPaint();
  }).observe(document.body, { subtree: true, childList: true, characterData: true, attributes: true });
}

const onRender = (_id: string, _phase: string, actualDuration: number) => {
  probe.renderMs += actualDuration;
  // El render que sigue a una respuesta cuenta aunque no cambie el DOM (mismos datos).
  if (probe.responseSinceCommit) {
    probe.responseSinceCommit = false;
    probe.lastActivity = now();
    markPaint();
  }
};

/** Envuelve la app en un `Profiler` que suma el tiempo de render de React. */
export function profiled(app: ReactNode): ReactNode {
  countRequests();
  return createElement(Profiler, { id: "app", onRender }, app);
}

async function quiet() {
  const start = now();
  while (now() - start < TIMEOUT_MS) {
    await frame();
    if (probe.pending === 0 && now() - probe.lastActivity >= QUIET_MS) return;
  }
  throw new Error("la interacción no terminó en 15 s");
}

interface Sample {
  ms: number;
  renderMs: number;
  requests: Record<string, number>;
}

async function measure(act: () => unknown): Promise<Sample> {
  await quiet();
  probe.requests = new Map();
  probe.renderMs = 0;
  const t0 = now();
  probe.lastPaint = t0;
  act();
  await quiet();
  return { ms: probe.lastPaint - t0, renderMs: probe.renderMs, requests: Object.fromEntries(probe.requests) };
}

const el = (perf: string) => {
  const e = document.querySelector<HTMLElement>(`[data-perf="${perf}"]`);
  if (!e) throw new Error(`sin [data-perf="${perf}"]`);
  return e;
};
const click = (perf: string) => () => el(perf).click();

let generation = 1_000_000;
/** Aviso de datos cambiados: deja frío lo ya cargado antes de medir un escenario en frío. */
const dataChanged = () => emit("data:changed", { generation: ++generation, agents: [], days: [] });

const stat = (xs: Sample[]) => ({ ...summarize(xs.map((x) => x.ms)), renderMs: summarize(xs.map((x) => x.renderMs)).median });

async function scenarios() {
  // S1: Resumen → Modelos → Resumen; la primera ida es en frío, el resto con lo ya visitado.
  const s1: Sample[] = [];
  for (let i = 0; i < REPS; i++) {
    s1.push(await measure(click("nav-model")));
    s1.push(await measure(click("nav-overview")));
  }
  // S2: 30 d → 7 d tras un aviso (frío) y vuelta a 30 d (cacheado).
  const s2cold: Sample[] = [];
  const s2cached: Sample[] = [];
  for (let i = 0; i < REPS; i++) {
    await measure(dataChanged);
    s2cold.push(await measure(click("period-7d")));
    s2cached.push(await measure(click("period-30d")));
  }
  // S3: ocultar un agente tras un aviso (frío); se vuelve a mostrar sin medir.
  const s3: Sample[] = [];
  el("agents").click();
  for (let i = 0; i < REPS; i++) {
    await measure(dataChanged);
    s3.push(await measure(click("agent-codex")));
    await measure(click("agent-codex"));
  }
  el("agents").click();
  // S4: aviso de datos cambiados con el Resumen abierto: peticiones hechas y render.
  const s4: Sample[] = [];
  for (let i = 0; i < REPS; i++) s4.push(await measure(dataChanged));
  return {
    S1: { cold: stat(s1.slice(0, 1)), cached: stat(s1.slice(1)) },
    S2: { cold: stat(s2cold), cached: stat(s2cached) },
    S3: { cold: stat(s3) },
    S4: { ...stat(s4), requests: s4[s4.length - 1].requests },
  };
}

/** Espera a la primera carga, mide y deja el JSON en el documento. */
export async function runHarness() {
  watchContent();
  const out = document.createElement("pre");
  out.id = "perf-results";
  try {
    while (probe.total === 0) await frame();
    await quiet();
    out.textContent = JSON.stringify(await scenarios());
  } catch (e) {
    out.textContent = JSON.stringify({ error: String(e) });
  }
  document.body.appendChild(out);
}
