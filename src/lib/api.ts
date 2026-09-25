import { invoke } from "@tauri-apps/api/core";

/** Filtro común; campos vacíos = vista general. Fechas en epoch ms UTC. */
export interface Filter {
  from?: number;
  to?: number;
  agents?: string[];
  projects?: number[];
}

export interface Summary {
  costUsd: number;
  calls: number;
  sessions: number;
  inputTokens: number;
  outputTokens: number;
  cacheRead: number;
  cacheWrite: number;
  cacheHit: number;
  cacheSavingsUsd: number;
  burnRateUsdH: number;
  firstTs: number | null;
  lastTs: number | null;
  unpricedModels: string[];
}

export interface Point {
  ts: number;
  costUsd: number;
  calls: number;
  sessions: number;
  inputTokens: number;
  outputTokens: number;
  cacheRead: number;
  cacheWrite: number;
}

export interface SeriesPoint {
  ts: number;
  key: string;
  label: string;
  costUsd: number;
  calls: number;
  outputTokens: number;
}

export interface BreakdownRow {
  key: string;
  label: string;
  costUsd: number;
  calls: number;
  errors: number;
  cacheHit: number;
  hasPrice: boolean;
  sessions: number;
  overheadTokens: number;
}

export type BreakdownBy = "agent" | "project" | "branch" | "model" | "tool" | "command" | "skill" | "mcp" | "agent_type";

export interface ActivityRow {
  key: string;
  costUsd: number;
  turns: number;
  editTurns: number;
  oneShot: number | null;
}

export interface ActivityReport {
  activities: ActivityRow[];
  models: { model: string; editTurns: number; oneShot: number | null }[];
}

export interface ActivityDay {
  ts: number;
  activity: string;
  costUsd: number;
  turns: number;
}

export interface AgentRow {
  id: string;
  name: string;
  logRoot: string;
  costUsd: number;
  calls: number;
}

export interface ProjectRow {
  id: number;
  name: string;
  cwd: string;
  costUsd: number;
  calls: number;
}

export interface DataInfo {
  firstTs: number | null;
  calls: number;
  watchedFiles: number;
  lastScan: number | null;
}

export type Theme = "system" | "light" | "dark";

export interface Settings {
  theme: Theme;
  monthlyBudget: number | null;
}

/** Minutos a sumar a UTC para obtener la hora local. */
export const tzOffsetMin = () => -new Date().getTimezoneOffset();

export const api = {
  summary: (filter: Filter) => invoke<Summary>("get_summary", { filter }),
  timeseries: (filter: Filter, bucket: "day" | "hour") =>
    invoke<Point[]>("get_timeseries", { filter, bucket, tzOffsetMin: tzOffsetMin() }),
  timeseriesBy: (filter: Filter, by: "agent" | "model" | "project" | "branch" | "tool") =>
    invoke<SeriesPoint[]>("get_timeseries_by", { filter, by, tzOffsetMin: tzOffsetMin() }),
  breakdown: (filter: Filter, by: BreakdownBy) => invoke<BreakdownRow[]>("get_breakdown", { filter, by }),
  activity: (filter: Filter) => invoke<ActivityReport>("get_activity", { filter }),
  activityDaily: (filter: Filter) => invoke<ActivityDay[]>("get_activity_daily", { filter, tzOffsetMin: tzOffsetMin() }),
  agents: (filter: Filter) => invoke<AgentRow[]>("list_agents", { filter }),
  projects: (filter: Filter) => invoke<ProjectRow[]>("list_projects", { filter }),
  dataInfo: () => invoke<DataInfo>("get_data_info"),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
};
