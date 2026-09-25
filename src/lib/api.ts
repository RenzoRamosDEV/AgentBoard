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
  firstTs: number | null;
  lastTs: number | null;
  unpricedModels: string[];
}

export const getSummary = (filter: Filter = {}) => invoke<Summary>("get_summary", { filter });
