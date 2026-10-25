#!/usr/bin/env node
/**
 * Medición de la interfaz (Fase 4): build `perf` con el mock y el arnés, `vite preview` y
 * Chromium headless (protocolo de DevTools, sin dependencias); repite cada latencia `--runs` veces y da mediana y máximo
 * de S1–S4. Imprime además el tamaño de los chunks de la build normal (bytes y gzip).
 *
 *   node scripts/perf-ui.mjs [--latency=104,630] [--runs=3] [--no-build]
 *
 * Chromium: `CHROME` en el entorno o el `chrome-headless-shell` de Playwright en `~/.cache`.
 */
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { gzipSync } from "node:zlib";
import { median, parseResults } from "../src/dev/perfStats.ts";

const ROOT = resolve(import.meta.dirname, "..");
const PERF_OUT = join(ROOT, "node_modules", ".perf-dist");
const PORT = 5199;

/** Bytes y gzip de cada `.js`/`.css` de `dir`, del mayor al menor. */
export function chunkSizes(dir) {
  return readdirSync(dir)
    .filter((f) => /\.(js|css)$/.test(f) && statSync(join(dir, f)).isFile())
    .map((file) => {
      const buf = readFileSync(join(dir, file));
      return { file, bytes: buf.length, gzip: gzipSync(buf).length };
    })
    .sort((a, b) => b.bytes - a.bytes);
}

function findChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  const base = join(homedir(), ".cache", "ms-playwright");
  const dirs = existsSync(base) ? readdirSync(base).filter((d) => d.startsWith("chromium_headless_shell-")).sort().reverse() : [];
  for (const d of dirs)
    for (const sub of readdirSync(join(base, d))) {
      const bin = join(base, d, sub, "chrome-headless-shell");
      if (existsSync(bin)) return bin;
    }
  throw new Error("sin chrome-headless-shell: define CHROME");
}

const vite = (args) => {
  const r = spawnSync("npx", ["vite", ...args], { cwd: ROOT, stdio: ["ignore", "ignore", "inherit"] });
  if (r.status !== 0) throw new Error(`vite ${args.join(" ")} falló`);
};

async function waitHttp(url) {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(url)).ok) return;
    } catch {
      // aún arrancando
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`${url} no responde`);
}

/** Chromium headless con el protocolo de DevTools (WebSocket de Node): espera al JSON del arnés. */
async function runOnce(chrome, latency) {
  const profile = mkdtempSync(join(tmpdir(), "perf-ui-chrome-"));
  const proc = spawn(chrome, ["--headless", "--no-sandbox", "--disable-gpu", "--window-size=1440,900", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "about:blank"], { stdio: ["ignore", "ignore", "pipe"] });
  try {
    const port = await new Promise((ok, ko) => {
      let buf = "";
      proc.stderr.on("data", (d) => {
        buf += d;
        const m = /DevTools listening on ws:\/\/[^:]+:(\d+)\//.exec(buf);
        if (m) ok(m[1]);
      });
      proc.on("exit", () => ko(new Error("Chromium terminó sin DevTools")));
    });
    const url = `http://localhost:${PORT}/?perf=1&latency=${latency}`;
    const target = await (await fetch(`http://127.0.0.1:${port}/json/new?${encodeURIComponent(url)}`, { method: "PUT" })).json();
    const ws = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((ok, ko) => ((ws.onopen = ok), (ws.onerror = ko)));
    let id = 0;
    const waiting = new Map();
    ws.onmessage = (e) => {
      const msg = JSON.parse(e.data);
      if (msg.method === "Runtime.exceptionThrown") console.error("[página]", msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text);
      waiting.get(msg.id)?.(msg.result);
    };
    const send = (method, params = {}) => new Promise((ok) => (waiting.set(++id, ok), ws.send(JSON.stringify({ id, method, params }))));
    await send("Runtime.enable");
    const deadline = Date.now() + 300_000;
    while (Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, 500));
      const r = await send("Runtime.evaluate", { expression: "document.documentElement.outerHTML", returnByValue: true });
      const html = r?.result?.value ?? "";
      if (!html.includes('id="perf-results"')) continue;
      ws.close();
      const res = parseResults(html);
      if (res.error) throw new Error(`arnés: ${res.error}`);
      return res;
    }
    throw new Error("el arnés no terminó en 5 min");
  } finally {
    proc.kill();
    rmSync(profile, { recursive: true, force: true });
  }
}

/** Une las ejecuciones: mediana de las medianas y máximo de los máximos de cada cifra. */
function combine(runs) {
  const walk = (objs) => {
    const first = objs[0];
    if (typeof first === "number") return median(objs);
    if (first === null || typeof first !== "object") return first;
    const out = {};
    for (const k of Object.keys(first)) {
      const vals = objs.map((o) => o[k]);
      out[k] = k === "max" ? Math.max(...vals) : k === "requests" ? vals[vals.length - 1] : walk(vals);
    }
    return out;
  };
  return walk(runs);
}

const fmtRow = (name, s) => `${name.padEnd(12)} mediana ${String(s.median).padStart(7)} ms   máx ${String(s.max).padStart(7)} ms   render ${String(Math.round(s.renderMs * 10) / 10).padStart(6)} ms`;

async function main() {
  const args = Object.fromEntries(process.argv.slice(2).map((a) => a.replace(/^--/, "").split("=")));
  const latencies = String(args.latency ?? "104").split(",").map(Number);
  const runs = Number(args.runs ?? 3);
  if (!("no-build" in args)) {
    vite(["build"]);
    vite(["build", "--mode", "perf", "--outDir", PERF_OUT, "--emptyOutDir"]);
  }
  const chrome = findChrome();
  const preview = spawn("npx", ["vite", "preview", "--port", String(PORT), "--strictPort", "--outDir", PERF_OUT], { cwd: ROOT, stdio: "ignore", detached: true });
  try {
    await waitHttp(`http://localhost:${PORT}/`);
    for (const latency of latencies) {
      const all = [];
      for (let i = 0; i < runs; i++) all.push(await runOnce(chrome, latency));
      const res = combine(all);
      console.log(`\n== Latencia ${latency} ms (${runs} ejecuciones × 5 repeticiones) ==`);
      console.log(fmtRow("S1 frío", res.S1.cold));
      console.log(fmtRow("S1 cacheado", res.S1.cached));
      console.log(fmtRow("S2 frío", res.S2.cold));
      console.log(fmtRow("S2 cacheado", res.S2.cached));
      console.log(fmtRow("S3 frío", res.S3.cold));
      console.log(fmtRow("S4 aviso", res.S4));
      console.log(`S4 peticiones: ${JSON.stringify(res.S4.requests)}`);
      console.log(`JSON ${latency}: ${JSON.stringify(res)}`);
    }
  } finally {
    process.kill(-preview.pid);
  }
  console.log("\n== Chunks (build normal, dist/assets) ==");
  let total = 0;
  let totalGz = 0;
  for (const c of chunkSizes(join(ROOT, "dist", "assets"))) {
    total += c.bytes;
    totalGz += c.gzip;
    console.log(`${c.file.padEnd(40)} ${String(c.bytes).padStart(9)} B ${String(c.gzip).padStart(8)} B gzip`);
  }
  console.log(`${"total".padEnd(40)} ${String(total).padStart(9)} B ${String(totalGz).padStart(8)} B gzip`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) await main();
