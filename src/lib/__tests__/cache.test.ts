import { describe, expect, it, vi } from "vitest";
import { createRebuild, isDirty } from "../cache";
import type { Settings } from "../api";

/** Fuente de eventos de prueba: guarda el oyente de `ingest://done` para dispararlo a mano. */
function fakeDone() {
  let handler: (() => void) | null = null;
  return {
    listen: (cb: () => void) => {
      handler = cb;
      return () => {
        handler = null;
      };
    },
    fire: () => handler?.(),
    listening: () => handler !== null,
  };
}

describe("reconstrucción de la caché", () => {
  it("queda en curso desde que se pulsa hasta que llega ingest://done", async () => {
    const done = fakeDone();
    const rebuild = vi.fn(() => Promise.resolve());
    const states: string[] = [];
    const r = createRebuild(rebuild, done.listen, (s) => states.push(s));
    await r.start();
    expect(rebuild).toHaveBeenCalledTimes(1);
    expect(r.state()).toBe("running");
    done.fire();
    expect(r.state()).toBe("idle");
    expect(states).toEqual(["running", "idle"]);
    expect(done.listening()).toBe(false);
  });

  it("no lanza otra mientras hay una en curso", async () => {
    const done = fakeDone();
    const rebuild = vi.fn(() => Promise.resolve());
    const r = createRebuild(rebuild, done.listen, () => {});
    await r.start();
    await r.start();
    expect(rebuild).toHaveBeenCalledTimes(1);
  });

  it("si el comando falla vuelve a estar disponible y da el error", async () => {
    const done = fakeDone();
    const states: string[] = [];
    const r = createRebuild(() => Promise.reject(new Error("sin hilo")), done.listen, (s) => states.push(s));
    await r.start();
    expect(r.state()).toBe("idle");
    expect(r.error()).toContain("sin hilo");
    expect(states).toEqual(["running", "idle"]);
    expect(done.listening()).toBe(false);
  });
});

describe("borrador de ajustes", () => {
  const base: Settings = {
    theme: "system",
    language: "system",
    monthlyBudget: null,
    dailyBudget: null,
    budgets: [],
    priceOverrides: [],
    alertAt80: true,
    alertAt100: true,
    trayShowsToday: false,
    memoryOnly: false,
  };

  it("cambiar «Solo memoria» deja algo por aplicar", () => {
    expect(isDirty(base, base)).toBe(false);
    expect(isDirty({ ...base, memoryOnly: true }, base)).toBe(true);
    expect(isDirty({ ...base, theme: "dark" }, base)).toBe(true);
    expect(isDirty({ ...base, language: "en" }, base)).toBe(true);
  });
});
