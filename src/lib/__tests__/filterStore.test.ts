import { describe, expect, it } from "vitest";
import { createFilterStore } from "../filterStore";
import { filterKey } from "../query";
import type { AgentRow, ProjectRow } from "../api";

function fakeStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v), data };
}

const agent = (id: string): AgentRow => ({ id, name: id, logRoot: "", costUsd: 0, calls: 0 });
const project = (id: number, name = `p${id}`): ProjectRow => ({ id, name, cwd: "", costUsd: 0, calls: 0 });
const agents = [agent("a"), agent("b")];
const projects = [project(1), project(2), project(3)];
const range = { from: 100 };

describe("almacén de filtro: acciones", () => {
  it("empieza en 30 días, sin ocultos", () => {
    const s = createFilterStore(fakeStorage()).get();
    expect(s.period).toEqual({ kind: "30d" });
    expect([...s.hiddenAgents]).toEqual([]);
    expect([...s.hiddenProjects]).toEqual([]);
  });

  it("toggleAgent oculta y vuelve a mostrar", () => {
    const store = createFilterStore(fakeStorage());
    store.toggleAgent("a");
    expect([...store.get().hiddenAgents]).toEqual(["a"]);
    store.toggleAgent("a");
    expect([...store.get().hiddenAgents]).toEqual([]);
  });

  it("toggleProject oculta y vuelve a mostrar", () => {
    const store = createFilterStore(fakeStorage());
    store.toggleProject(2);
    expect([...store.get().hiddenProjects]).toEqual([2]);
    store.toggleProject(2);
    expect([...store.get().hiddenProjects]).toEqual([]);
  });

  it("onlyProject oculta todos los demás y con null los muestra todos", () => {
    const store = createFilterStore(fakeStorage());
    store.onlyProject(2, [1, 2, 3]);
    expect([...store.get().hiddenProjects]).toEqual([1, 3]);
    store.onlyProject(null, [1, 2, 3]);
    expect([...store.get().hiddenProjects]).toEqual([]);
  });

  it("setPeriod cambia el periodo", () => {
    const store = createFilterStore(fakeStorage());
    store.setPeriod({ kind: "7d" });
    expect(store.get().period).toEqual({ kind: "7d" });
  });

  it("setCompare cambia la comparación y la guarda", () => {
    const storage = fakeStorage();
    const store = createFilterStore(storage);
    store.setCompare(false);
    expect(store.get().compare).toBe(false);
    expect(storage.data.get("agentboard.compare")).toBe("0");
    store.setCompare(true);
    expect(storage.data.get("agentboard.compare")).toBe("1");
  });
});

describe("almacén de filtro: comparación guardada", () => {
  it.each([
    ["0", false],
    ["1", true],
    [undefined, true],
  ])("con %s guardado vale %s", (saved, expected) => {
    const store = createFilterStore(fakeStorage(saved === undefined ? {} : { "agentboard.compare": saved }));
    expect(store.get().compare).toBe(expected);
  });

  it("sin almacenamiento vale true y cambiarla no falla", () => {
    const broken = {
      getItem: () => {
        throw new Error("sin almacenamiento");
      },
      setItem: () => {
        throw new Error("sin almacenamiento");
      },
    };
    const store = createFilterStore(broken);
    expect(store.get().compare).toBe(true);
    store.setCompare(false);
    expect(store.get().compare).toBe(false);
  });
});

describe("almacén de filtro: suscripciones", () => {
  it("un oyente de un selector no se llama si cambia otra parte del estado", () => {
    const store = createFilterStore(fakeStorage());
    let periodCalls = 0;
    let agentCalls = 0;
    store.watch((s) => s.period, () => periodCalls++);
    store.watch((s) => s.hiddenAgents, () => agentCalls++);
    store.toggleAgent("a");
    expect(periodCalls).toBe(0);
    expect(agentCalls).toBe(1);
    store.setPeriod({ kind: "7d" });
    expect(periodCalls).toBe(1);
    expect(agentCalls).toBe(1);
  });

  it("deja de avisar tras darse de baja", () => {
    const store = createFilterStore(fakeStorage());
    let calls = 0;
    const off = store.subscribe(() => calls++);
    store.setPeriod({ kind: "7d" });
    off();
    store.setPeriod({ kind: "90d" });
    expect(calls).toBe(1);
  });
});

describe("almacén de filtro: filtro derivado", () => {
  it("sin ocultos no filtra agentes ni proyectos", () => {
    const d = createFilterStore(fakeStorage()).derive(range, agents, projects);
    expect(d.filter).toEqual({ from: 100, agents: undefined, projects: undefined });
    expect(d.singleProject).toBeNull();
  });

  it("con ocultos lista los visibles y con un solo proyecto visible da su nombre", () => {
    const store = createFilterStore(fakeStorage());
    store.toggleAgent("a");
    store.onlyProject(2, [1, 2, 3]);
    const d = store.derive(range, agents, projects);
    expect(d.filter).toEqual({ from: 100, agents: ["b"], projects: [2] });
    expect(d.singleProject).toBe("p2");
  });

  it("es el mismo objeto mientras no cambian sus entradas, aunque las listas lleguen de nuevo iguales", () => {
    const store = createFilterStore(fakeStorage());
    store.toggleAgent("a");
    const d1 = store.derive(range, agents, projects);
    const d2 = store.derive({ from: 100 }, [agent("a"), agent("b")], [project(1), project(2), project(3)]);
    expect(d2).toBe(d1);
    expect(d2.key).toBe(filterKey(d1.filter));
  });

  it("cambia si cambia el rango", () => {
    const store = createFilterStore(fakeStorage());
    const d1 = store.derive(range, agents, projects);
    const d2 = store.derive({ from: 200 }, agents, projects);
    expect(d2).not.toBe(d1);
    expect(d2.key).not.toBe(d1.key);
  });

  it("prevFilter es null con «Todo»", () => {
    const store = createFilterStore(fakeStorage());
    store.setPeriod({ kind: "all" });
    expect(store.derive({}, agents, projects).prevFilter).toBeNull();
  });

  it("prevFilter es null sin comparación", () => {
    const store = createFilterStore(fakeStorage());
    store.setCompare(false);
    expect(store.derive(range, agents, projects).prevFilter).toBeNull();
  });

  it("prevFilter lleva el periodo anterior con los mismos agentes y proyectos", () => {
    const store = createFilterStore(fakeStorage());
    store.toggleAgent("a");
    const d = store.derive(range, agents, projects, new Date(2026, 9, 25, 12));
    expect(d.prevFilter).toEqual({
      from: new Date(2026, 7, 27).getTime(),
      to: new Date(2026, 8, 26).getTime(),
      agents: ["b"],
      projects: undefined,
    });
  });

  it("current() devuelve el último derivado", () => {
    const store = createFilterStore(fakeStorage());
    const d = store.derive(range, agents, projects);
    expect(store.current()).toBe(d);
  });
});
