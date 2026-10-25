import { describe, expect, it, vi } from "vitest";
import { DATA_CHANGED, INGEST_DONE, PANEL_SHOWN, subscribeApp, subscribeTray, whenVisible, type Listen } from "../live";
import { readFileSync } from "node:fs";
import { createQueryStore } from "../query";

const source = (rel: string) => readFileSync(new URL(rel, import.meta.url), "utf8");
const trayPanelSource = source("../../views/TrayPanel.tsx");
const settingsDialogSource = source("../../components/SettingsDialog.tsx");

/** Bus de eventos de prueba con la forma de `listen` de Tauri. */
function fakeBus() {
  const handlers = new Map<string, Set<(e: { payload: unknown }) => void>>();
  const listen: Listen = (name, cb) => {
    const set = handlers.get(name) ?? new Set();
    set.add(cb as (e: { payload: unknown }) => void);
    handlers.set(name, set);
    return Promise.resolve(() => {
      set.delete(cb as (e: { payload: unknown }) => void);
    });
  };
  return {
    listen,
    emit: (name: string, payload: unknown = null) => handlers.get(name)?.forEach((h) => h({ payload })),
    listening: (name: string) => handlers.get(name)?.size ?? 0,
  };
}

const changed = { generation: 3, agents: ["claude-code"], days: [20260901] };

describe("suscripción de la ventana principal", () => {
  it("cada aviso cambia la generación de la caché y resalta solo tras el primer ingest://done", () => {
    const bus = fakeBus();
    const store = createQueryStore({ now: () => 0 });
    const h = { changed: vi.fn(), live: vi.fn() };
    subscribeApp(bus.listen, store, h);
    bus.emit(DATA_CHANGED, changed);
    expect(store.generation()).toBe(3);
    expect(h.changed).toHaveBeenCalledTimes(1);
    expect(h.live).not.toHaveBeenCalled();
    bus.emit(INGEST_DONE, { files: 1, lines: 1, records: 1, errors: 0, full: true });
    expect(store.generation()).toBe(4);
    expect(h.changed).toHaveBeenCalledTimes(2);
    expect(h.live).not.toHaveBeenCalled();
    bus.emit(DATA_CHANGED, { ...changed, generation: 4 });
    expect(store.generation()).toBe(5);
    expect(h.changed).toHaveBeenCalledTimes(3);
    expect(h.live).toHaveBeenCalledTimes(1);
  });

  it("deja de escuchar al soltarse", async () => {
    const bus = fakeBus();
    const off = subscribeApp(bus.listen, createQueryStore({ now: () => 0 }), { changed: vi.fn(), live: vi.fn() });
    expect(bus.listening(DATA_CHANGED)).toBe(1);
    off();
    await Promise.resolve();
    await Promise.resolve();
    expect(bus.listening(DATA_CHANGED)).toBe(0);
    expect(bus.listening(INGEST_DONE)).toBe(0);
  });
});

describe("suscripción del panel de la bandeja", () => {
  it("refresca con data:changed y al abrirse, no con ingest://done", () => {
    const bus = fakeBus();
    const bump = vi.fn();
    subscribeTray(bus.listen, bump);
    bus.emit(INGEST_DONE, {});
    expect(bump).not.toHaveBeenCalled();
    bus.emit(DATA_CHANGED, changed);
    bus.emit(PANEL_SHOWN);
    expect(bump).toHaveBeenCalledTimes(2);
  });

  it("el panel no programa intervalos ni escucha ingest://done", () => {
    expect(trayPanelSource).not.toMatch(/setInterval/);
    expect(trayPanelSource).not.toMatch(/ingest:\/\/done/);
  });

  it("reconstruir la caché sigue terminando con ingest://done", () => {
    expect(settingsDialogSource).toMatch(/listen\("ingest:\/\/done"/);
  });
});

describe("Hoy en directo en reposo", () => {
  it("no consulta con el documento oculto", () => {
    const fn = vi.fn();
    whenVisible({ visibilityState: "hidden" }, fn);
    expect(fn).not.toHaveBeenCalled();
    whenVisible({ visibilityState: "visible" }, fn);
    expect(fn).toHaveBeenCalledTimes(1);
  });
});
