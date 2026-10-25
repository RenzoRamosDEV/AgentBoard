import { describe, expect, it, vi } from "vitest";
import { dashboardKey, loadDashboard } from "../useData";
import type { Filter } from "../api";

const snapshot = (withPrev: boolean) => ({
  summary: { costUsd: 1 },
  agents: [],
  daily: [],
  hourly: [],
  dailyByAgent: [],
  dailyByModel: [],
  dailyByProject: [],
  dailyByTool: [],
  month: [],
  projects: [],
  branches: null,
  models: [],
  activity: { activities: [], models: [] },
  activityDaily: [],
  tools: [],
  commands: [],
  skills: [],
  mcp: [],
  agentTypes: [],
  prev: withPrev ? { summary: { costUsd: 2 }, daily: [], projects: [] } : null,
});

describe("datos del Resumen", () => {
  it("una sola petición con el mes, la vista por ramas y el periodo anterior", async () => {
    const filter: Filter = { from: 10, to: 20, agents: ["claude-code"] };
    const prev: Filter = { from: 0, to: 10, agents: ["claude-code"] };
    const invoke = vi.fn(async () => snapshot(true));
    const data = await loadDashboard(invoke, filter, "proyecto", prev, 5);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("get_dashboard_snapshot", {
      request: { filter, tz: expect.any(String), monthFrom: 5, branchView: true, previous: prev },
    });
    expect(data.filter).toBe(filter);
    expect(data.summary.costUsd).toBe(1);
    expect(data.prev?.filter).toBe(prev);
    expect(data.prev?.summary.costUsd).toBe(2);
  });

  it("sin comparación ni proyecto único", async () => {
    const filter: Filter = {};
    const invoke = vi.fn(async () => snapshot(false));
    const data = await loadDashboard(invoke, filter, null, null, 7);
    expect(invoke).toHaveBeenCalledWith("get_dashboard_snapshot", {
      request: { filter, tz: expect.any(String), monthFrom: 7, branchView: false, previous: null },
    });
    expect(data.prev).toBeNull();
    expect(data.branches).toBeNull();
  });
});

describe("clave del Resumen en la caché", () => {
  const f: Filter = { from: 10, agents: ["a"] };
  const prev: Filter = { from: 0, to: 10, agents: ["a"] };

  it("es la misma para el mismo filtro, proyecto único, periodo anterior y mes", () => {
    expect(dashboardKey({ agents: ["a"], from: 10 }, null, prev, 5)).toBe(dashboardKey(f, null, prev, 5));
  });

  it.each([
    ["el filtro", dashboardKey({ from: 11, agents: ["a"] }, null, prev, 5)],
    ["el proyecto único", dashboardKey(f, "p", prev, 5)],
    ["la comparación", dashboardKey(f, null, null, 5)],
    ["el mes", dashboardKey(f, null, prev, 6)],
  ])("cambia con %s", (_, other) => {
    expect(other).not.toBe(dashboardKey(f, null, prev, 5));
  });
});
