import { describe, expect, it, vi } from "vitest";
import { loadDashboard } from "../useData";
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
