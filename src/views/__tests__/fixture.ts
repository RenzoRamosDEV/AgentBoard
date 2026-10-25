/** Datos del Resumen deterministas para los golden de las vistas (sin reloj ni aleatoriedad). */
import type { ActivityDay, BreakdownRow, Point, SeriesPoint } from "../../lib/api";
import type { DashboardData } from "../../lib/useData";

export const NOW = new Date(2026, 9, 25, 12, 0, 0).getTime();
const DAY = 864e5;
const day = (i: number) => new Date(2026, 9, 25 - i).getTime();

const row = (key: string, label: string, costUsd: number, calls: number, errors = 0, sessions = 0, agent?: string): BreakdownRow => ({
  key, label, costUsd, calls, errors, cacheHit: 0.8 + (calls % 7) / 100, hasPrice: costUsd > 0 || calls % 2 === 0, sessions, overheadTokens: sessions * 1000, ...(agent ? { agent } : {}),
});

function points(days: number, from: number): Point[] {
  const out: Point[] = [];
  for (let i = days - 1; i >= 0; i--) {
    if (i % 5 === 3) continue;
    const ts = day(i) + (from ? 0 : 0);
    out.push({ ts, costUsd: 1 + ((i * 37) % 11) * 0.5, calls: 80 + (i % 7) * 10, sessions: 1 + (i % 3), inputTokens: 1000 * i, outputTokens: 500 * i, cacheRead: 9000 * i, cacheWrite: 400 * i });
  }
  return out;
}

function series(days: number, keys: [string, string][]): SeriesPoint[] {
  const out: SeriesPoint[] = [];
  for (let i = days - 1; i >= 0; i--)
    keys.forEach(([key, label], k) => {
      if ((i + k) % 4 === 3) return;
      out.push({ ts: day(i), key, label, costUsd: ((1 + ((i * 13) % 7)) * 0.7) / (k + 1), calls: 30 + ((i + k) % 9) * 5, outputTokens: 2000 * (k + 1) + i });
    });
  return out;
}

const keysOf = (prefix: string, n: number): [string, string][] => Array.from({ length: n }, (_, i) => [`${prefix}${i}`, `${prefix}-${i}`]);

export function fixture(days: number): DashboardData {
  const from = day(days - 1);
  const daily = points(days, from);
  const activityDaily: ActivityDay[] = [];
  for (let i = days - 1; i >= 0; i -= 2) for (const [k, a] of ["coding", "testing", "exploration", "debugging"].entries()) activityDaily.push({ ts: day(i), activity: a, costUsd: 0.3 + ((i + k) % 5) * 0.4, turns: 2 + k });
  return {
    filter: { from },
    summary: { costUsd: 123.45, calls: 4567, sessions: 89, inputTokens: 12345, outputTokens: 678900, cacheRead: 120_000_000, cacheWrite: 5_000_000, cacheHit: 0.93, cacheSavingsUsd: 321.5, burnRateUsdH: 1.25, firstTs: from, lastTs: NOW, unpricedModels: ["local"] },
    agents: [row("claude-code", "Claude Code", 90, 3000, 3, 60), row("codex", "Codex CLI", 30, 1200, 0, 20), row("opencode", "OpenCode", 3.45, 367, 1, 9)],
    daily,
    hourly: Array.from({ length: 48 }, (_, i) => ({ ts: NOW - i * 3_600_000, costUsd: (i % 6) * 0.2, calls: i, sessions: 1, inputTokens: 0, outputTokens: 0, cacheRead: 0, cacheWrite: 0 })),
    dailyByAgent: series(days, [["claude-code", "Claude Code"], ["codex", "Codex CLI"], ["opencode", "OpenCode"]]),
    dailyByModel: series(days, keysOf("model", 10)),
    dailyByProject: series(days, keysOf("proj", 12)),
    dailyByTool: series(days, keysOf("tool", 6)),
    month: points(25, day(24)),
    projects: keysOf("proj", 12).map(([k, l], i) => row(k, l, 40 / (i + 1), 100 * (12 - i), i % 3, 12 - i)),
    branches: null,
    models: keysOf("model", 10).map(([k, l], i) => row(k, l, 50 / (i + 1), 300 + i * 17, i % 2, 0)),
    activity: {
      activities: ["coding", "testing", "exploration", "debugging", "feature", "conversation", "build", "delegation", "brainstorming", "other"].map((key, i) => ({ key, costUsd: 30 / (i + 1), turns: 100 - i * 9, editTurns: i % 3 ? 0 : 50 - i, oneShot: i % 3 ? null : 0.9 - i / 50 })),
      models: [{ model: "model0", editTurns: 120, oneShot: 0.93 }, { model: "model1", editTurns: 40, oneShot: 0.81 }],
    },
    activityDaily,
    tools: keysOf("tool", 9).map(([k, l], i) => ({ ...row(k, l, 0, 900 - i * 80, i * 3), costUsd: 0 })),
    commands: keysOf("cmd", 9).map(([k, l], i) => ({ ...row(k, l, 0, 400 - i * 30, i), costUsd: 0 })),
    skills: keysOf("skill", 9).map(([k, l], i) => row(k, l, 0.5 / (i + 1), 10 - i)),
    mcp: keysOf("mcp", 4).map(([k, l], i) => row(k, l, 0, 20 - i * 3)),
    agentTypes: keysOf("type", 9).map(([k, l], i) => row(`claude-code:${k}`, l, 20 / (i + 1), 200 - i * 10, 0, 0, i % 2 ? "codex" : "claude-code")),
    prev: { filter: { from: from - days * DAY, to: from }, summary: { costUsd: 100, calls: 4000, sessions: 80, inputTokens: 10000, outputTokens: 600000, cacheRead: 100_000_000, cacheWrite: 4_000_000, cacheHit: 0.9, cacheSavingsUsd: 300, burnRateUsdH: 0, firstTs: from - days * DAY, lastTs: from, unpricedModels: [] }, daily: points(days, from).map((p) => ({ ...p, ts: p.ts - days * DAY })), projects: keysOf("proj", 5).map(([k, l], i) => row(k, l, 30 / (i + 1), 100, 0, 5)) },
  };
}
