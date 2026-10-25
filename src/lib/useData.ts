import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { tzName, type ActivityDay, type ActivityReport, type BreakdownRow, type Filter, type Point, type SeriesPoint, type Summary } from "./api";
import { monthStart } from "./period";
import { filterKey, useQuery } from "./query";

export interface DashboardData {
  /** Filtro con el que se cargó (para rellenar días sin actividad). */
  filter: Filter;
  summary: Summary;
  agents: BreakdownRow[];
  daily: Point[];
  /** Coste por hora local, para el reparto por hora del día. */
  hourly: Point[];
  dailyByAgent: SeriesPoint[];
  dailyByModel: SeriesPoint[];
  /** Por proyecto, o por rama si hay un solo proyecto seleccionado. */
  dailyByProject: SeriesPoint[];
  dailyByTool: SeriesPoint[];
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
  /** Periodo anterior, solo con la comparación activa. */
  prev: PrevData | null;
}

export interface PrevData {
  filter: Filter;
  summary: Summary;
  daily: Point[];
  projects: BreakdownRow[];
}

/** Lo que devuelve `get_dashboard_snapshot`: los apartados sin los filtros. */
export type DashboardSnapshot = Omit<DashboardData, "filter" | "prev"> & { prev: Omit<PrevData, "filter"> | null };

type Invoke = (cmd: string, args: Record<string, unknown>) => Promise<unknown>;

/**
 * Todos los apartados del Resumen en una sola petición, sobre un mismo estado de los datos.
 * `monthFrom` es el inicio del mes en curso en la hora local (el gasto del mes ignora el periodo).
 */
export async function loadDashboard(
  invoke: Invoke,
  filter: Filter,
  singleProject: string | null,
  prevFilter: Filter | null,
  monthFrom: number,
): Promise<DashboardData> {
  const snap = (await invoke("get_dashboard_snapshot", {
    request: { filter, tz: tzName(), monthFrom, branchView: !!singleProject, previous: prevFilter },
  })) as DashboardSnapshot;
  return { ...snap, filter, prev: snap.prev && prevFilter ? { ...snap.prev, filter: prevFilter } : null };
}

/** Clave del Resumen en la caché de consultas. */
export function dashboardKey(filter: Filter, singleProject: string | null, prevFilter: Filter | null, monthFrom: number): string {
  return `${filterKey(filter)}|${singleProject ?? ""}|${prevFilter ? filterKey(prevFilter) : "-"}|${monthFrom}`;
}

/** Carga todos los apartados para el filtro activo (de la caché si ya están). */
export function useDashboardData(filter: Filter, singleProject: string | null, prevFilter: Filter | null = null) {
  const monthFrom = monthStart().getTime();
  const { data, error } = useQuery("get_dashboard_snapshot", dashboardKey(filter, singleProject, prevFilter, monthFrom), () =>
    loadDashboard(tauriInvoke, filter, singleProject, prevFilter, monthFrom),
  );
  return { data: data ?? null, error };
}
