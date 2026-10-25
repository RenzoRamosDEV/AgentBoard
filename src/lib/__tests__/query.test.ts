import { describe, expect, it } from "vitest";
import { createQueryStore, filterKey } from "../query";

/** Petición falsa: cada llamada queda pendiente hasta resolverla o fallarla a mano. */
function fakeFetch<T>() {
  const calls: { resolve: (v: T) => void; reject: (e: unknown) => void }[] = [];
  const fetch = () => new Promise<T>((resolve, reject) => calls.push({ resolve, reject }));
  return { fetch, calls };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

function clock(start = 1_000) {
  let t = start;
  return { now: () => t, advance: (ms: number) => (t += ms) };
}

const noop = () => {};

describe("caché de consultas", () => {
  it("dos suscripciones simultáneas a la misma clave hacen una sola petición y ven la misma respuesta", async () => {
    const store = createQueryStore({ now: clock().now });
    const f = fakeFetch<string>();
    store.subscribe("a", f.fetch, noop);
    store.subscribe("a", f.fetch, noop);
    expect(f.calls).toHaveLength(1);
    f.calls[0].resolve("x");
    await flush();
    expect(store.snapshot("a").data).toBe("x");
  });

  it("una entrada fresca no se vuelve a pedir al suscribirse de nuevo", async () => {
    const store = createQueryStore({ now: clock().now });
    const f = fakeFetch<string>();
    const off = store.subscribe("a", f.fetch, noop);
    f.calls[0].resolve("x");
    await flush();
    off();
    store.subscribe("a", f.fetch, noop);
    expect(f.calls).toHaveLength(1);
    expect(store.snapshot("a").data).toBe("x");
  });

  it("avisa a los suscriptores cuando llega la respuesta", async () => {
    const store = createQueryStore({ now: clock().now });
    const f = fakeFetch<string>();
    let notified = 0;
    store.subscribe("a", f.fetch, () => notified++);
    f.calls[0].resolve("x");
    await flush();
    expect(notified).toBe(1);
  });

  it("una clave sin pedir no tiene datos ni error", () => {
    const store = createQueryStore({ now: clock().now });
    expect(store.snapshot("nada")).toEqual({ data: undefined, error: null });
  });

  it("setGeneration vuelve a pedir solo las claves con suscriptores; las demás, al suscribirse, mostrando antes el dato anterior", async () => {
    const store = createQueryStore({ now: clock().now });
    const mounted = fakeFetch<string>();
    const hidden = fakeFetch<string>();
    store.subscribe("montada", mounted.fetch, noop);
    const off = store.subscribe("oculta", hidden.fetch, noop);
    mounted.calls[0].resolve("m1");
    hidden.calls[0].resolve("o1");
    await flush();
    off();

    store.setGeneration(1);
    expect(mounted.calls).toHaveLength(2);
    expect(hidden.calls).toHaveLength(1);

    store.subscribe("oculta", hidden.fetch, noop);
    expect(hidden.calls).toHaveLength(2);
    expect(store.snapshot("oculta").data).toBe("o1");
    hidden.calls[1].resolve("o2");
    await flush();
    expect(store.snapshot("oculta").data).toBe("o2");
  });

  it("cada setGeneration cambia la generación aunque repita el número", () => {
    const store = createQueryStore({ now: clock().now });
    store.setGeneration(3);
    expect(store.generation()).toBe(3);
    store.setGeneration(3);
    expect(store.generation()).toBe(4);
    store.setGeneration(2);
    expect(store.generation()).toBe(5);
  });

  it("avisa a los oyentes de la generación", () => {
    const store = createQueryStore({ now: clock().now });
    let seen = 0;
    const off = store.onGeneration(() => seen++);
    store.setGeneration(1);
    off();
    store.setGeneration(2);
    expect(seen).toBe(1);
  });

  it("la respuesta de una generación superada no pisa la nueva", async () => {
    const store = createQueryStore({ now: clock().now });
    const f = fakeFetch<string>();
    store.subscribe("a", f.fetch, noop);
    store.setGeneration(1);
    expect(f.calls).toHaveLength(2);
    f.calls[1].resolve("nuevo");
    await flush();
    f.calls[0].resolve("viejo");
    await flush();
    expect(store.snapshot("a").data).toBe("nuevo");
  });

  it("invalidateAll vuelve a pedir las montadas y deja obsoletas las demás", async () => {
    const store = createQueryStore({ now: clock().now });
    const mounted = fakeFetch<string>();
    const hidden = fakeFetch<string>();
    store.subscribe("montada", mounted.fetch, noop);
    const off = store.subscribe("oculta", hidden.fetch, noop);
    mounted.calls[0].resolve("m");
    hidden.calls[0].resolve("o");
    await flush();
    off();
    store.invalidateAll();
    expect(mounted.calls).toHaveLength(2);
    expect(hidden.calls).toHaveLength(1);
    store.subscribe("oculta", hidden.fetch, noop);
    expect(hidden.calls).toHaveLength(2);
  });

  it.each([
    [599_999, 1],
    [600_000, 2],
    [600_001, 2],
  ])("con %i ms de antigüedad hace %i peticiones en total", async (age, expected) => {
    const c = clock();
    const store = createQueryStore({ now: c.now });
    const f = fakeFetch<string>();
    const off = store.subscribe("a", f.fetch, noop);
    f.calls[0].resolve("x");
    await flush();
    off();
    c.advance(age);
    store.subscribe("a", f.fetch, noop);
    expect(f.calls).toHaveLength(expected);
  });

  it("con maxEntries 2 descarta la entrada menos usada sin suscriptores y conserva las montadas", async () => {
    const store = createQueryStore({ now: clock().now, maxEntries: 2 });
    const f = fakeFetch<string>();
    store.subscribe("montada", f.fetch, noop);
    const offA = store.subscribe("a", f.fetch, noop);
    f.calls.forEach((c, i) => c.resolve(`v${i}`));
    await flush();
    offA();
    const offB = store.subscribe("b", f.fetch, noop);
    f.calls[2].resolve("vb");
    await flush();
    offB();
    expect(store.snapshot("montada").data).toBe("v0");
    expect(store.snapshot("a").data).toBeUndefined();
    expect(store.snapshot("b").data).toBe("vb");
  });

  it("al caducar dentro de la misma generación, la nueva respuesta sustituye a la anterior", async () => {
    const c = clock();
    const store = createQueryStore({ now: c.now });
    const f = fakeFetch<string>();
    const off = store.subscribe("a", f.fetch, noop);
    f.calls[0].resolve("viejo");
    await flush();
    off();
    c.advance(600_000);
    store.subscribe("a", f.fetch, noop);
    f.calls[1].resolve("nuevo");
    await flush();
    expect(store.snapshot("a").data).toBe("nuevo");
  });

  it("un error en la misma generación se guarda y el siguiente suscriptor reintenta", async () => {
    const c = clock();
    const store = createQueryStore({ now: c.now });
    const f = fakeFetch<string>();
    const off = store.subscribe("a", f.fetch, noop);
    f.calls[0].resolve("x");
    await flush();
    off();
    c.advance(600_000);
    const off2 = store.subscribe("a", f.fetch, noop);
    f.calls[1].reject(new Error("caído"));
    await flush();
    expect(store.snapshot("a")).toEqual({ data: "x", error: "Error: caído" });
    off2();
    store.subscribe("a", f.fetch, noop);
    expect(f.calls).toHaveLength(3);
  });

  it("con tantas entradas como el límite no descarta ninguna", async () => {
    const store = createQueryStore({ now: clock().now, maxEntries: 2 });
    const f = fakeFetch<string>();
    store.subscribe("a", f.fetch, noop)();
    store.subscribe("b", f.fetch, noop)();
    f.calls.forEach((c, i) => c.resolve(`v${i}`));
    await flush();
    expect(store.snapshot("a").data).toBe("v0");
    expect(store.snapshot("b").data).toBe("v1");
  });

  it("descarta la usada hace más tiempo, no la primera en entrar", async () => {
    const store = createQueryStore({ now: clock().now, maxEntries: 2 });
    const f = fakeFetch<string>();
    store.subscribe("a", f.fetch, noop)();
    store.subscribe("b", f.fetch, noop)();
    f.calls.forEach((c, i) => c.resolve(`v${i}`));
    await flush();
    store.subscribe("a", f.fetch, noop)();
    store.subscribe("c", f.fetch, noop)();
    expect(store.snapshot("a").data).toBe("v0");
    expect(store.snapshot("b").data).toBeUndefined();
  });

  it("un error se guarda sin perder el dato anterior y se borra con la siguiente respuesta", async () => {
    const store = createQueryStore({ now: clock().now });
    const f = fakeFetch<string>();
    store.subscribe("a", f.fetch, noop);
    f.calls[0].resolve("x");
    await flush();
    store.setGeneration(1);
    f.calls[1].reject(new Error("caído"));
    await flush();
    expect(store.snapshot("a")).toEqual({ data: "x", error: "Error: caído" });
    store.setGeneration(2);
    f.calls[2].resolve("y");
    await flush();
    expect(store.snapshot("a")).toEqual({ data: "y", error: null });
  });

  it("la instantánea es el mismo objeto mientras no cambia la entrada", async () => {
    const store = createQueryStore({ now: clock().now });
    const f = fakeFetch<string>();
    store.subscribe("a", f.fetch, noop);
    f.calls[0].resolve("x");
    await flush();
    expect(store.snapshot("a")).toBe(store.snapshot("a"));
  });
});

describe("clave de un filtro", () => {
  const base = { from: 1, to: 2, agents: ["a", "b"], projects: [1, 2] };

  it("es la misma para filtros iguales con las propiedades en otro orden", () => {
    expect(filterKey({ projects: [1, 2], agents: ["a", "b"], to: 2, from: 1 })).toBe(filterKey(base));
  });

  it.each([
    ["from", { ...base, from: 3 }],
    ["to", { ...base, to: undefined }],
    ["agents", { ...base, agents: ["a"] }],
    ["projects", { ...base, projects: [1] }],
    ["agents vacío frente a sin filtro", { ...base, agents: [] as string[] }],
  ])("cambia si cambia %s", (_, other) => {
    expect(filterKey(other)).not.toBe(filterKey(base));
  });

  it("sin filtro de agentes no es lo mismo que ninguno", () => {
    expect(filterKey({ agents: [] })).not.toBe(filterKey({}));
  });
});
