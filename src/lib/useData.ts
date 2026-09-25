import { useEffect, useState } from "react";
import { api, type ActivityDay, type ActivityReport, type BreakdownRow, type Filter, type Point, type Summary } from "./api";
import { monthStart } from "./period";

export interface DashboardData {
  summary: Summary;
  daily: Point[];
  month: Point[];
  projects: BreakdownRow[];
  /** Solo con un proyecto seleccionado. */
  branches: BreakdownRow[] | null;
  models: BreakdownRow[];
  activity: ActivityReport;
  activityDaily: ActivityDay[];
  tools: BreakdownRow[];
  commands: BreakdownRow[];
  skills: BreakdownRow[];
  mcp: BreakdownRow[];
  agentTypes: BreakdownRow[];
}

/** Carga todos los apartados para el filtro activo. */
export function useDashboardData(filter: Filter, singleProject: string | null, refresh: number) {
  const [data, setData] = useState<DashboardData | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    // Gasto del mes: respeta agentes y proyectos, ignora el periodo.
    const monthFilter: Filter = { ...filter, from: monthStart().getTime(), to: undefined };
    const by = (k: Parameters<typeof api.breakdown>[1]) => api.breakdown(filter, k);
    Promise.all([
      api.summary(filter),
      api.timeseries(filter, "day"),
      api.timeseries(monthFilter, "day"),
      by("project"),
      singleProject ? by("branch") : Promise.resolve(null),
      by("model"),
      api.activity(filter),
      api.activityDaily(filter),
      by("tool"),
      by("command"),
      by("skill"),
      by("mcp"),
      by("agent_type"),
    ])
      .then(([summary, daily, month, projects, branches, models, activity, activityDaily, tools, commands, skills, mcp, agentTypes]) => {
        if (!alive) return;
        setData({ summary, daily, month, projects, branches, models, activity, activityDaily, tools, commands, skills, mcp, agentTypes });
        setError(null);
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [filter, singleProject, refresh]);

  return { data, error };
}
