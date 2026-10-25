/**
 * Datos de ejemplo para desarrollar la UI en un navegador normal (`npm run dev`),
 * sin el núcleo Rust. Solo se carga en modo dev y fuera de Tauri.
 */
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

const DAY = 864e5;
const now = Date.now();
const monthStart = new Date(new Date().getFullYear(), new Date().getMonth(), 1).getTime();

const models = [
  { key: "claude-fable-5-1", label: "claude-fable-5-1", costUsd: 62.1, calls: 742, errors: 0, cacheHit: 0.971, hasPrice: true, sessions: 0, overheadTokens: 0 },
  { key: "claude-opus-5-5", label: "claude-opus-5-5", costUsd: 41.2, calls: 1840, errors: 0, cacheHit: 0.91, hasPrice: true, sessions: 0, overheadTokens: 0 },
  { key: "claude-sonnet-5", label: "claude-sonnet-5", costUsd: 9.8, calls: 1210, errors: 0, cacheHit: 0.87, hasPrice: true, sessions: 0, overheadTokens: 0 },
  { key: "gpt-5-codex", label: "gpt-5-codex", costUsd: 4.1, calls: 320, errors: 0, cacheHit: 0.62, hasPrice: true, sessions: 0, overheadTokens: 0 },
  { key: "claude-haiku-4-5", label: "claude-haiku-4-5", costUsd: 0.9, calls: 410, errors: 0, cacheHit: 0.8, hasPrice: true, sessions: 0, overheadTokens: 0 },
  { key: "modelo-local", label: "modelo-local", costUsd: 0, calls: 12, errors: 0, cacheHit: 0, hasPrice: false, sessions: 0, overheadTokens: 0 },
];
const row = (label: string, costUsd: number, calls: number, errors = 0, sessions = 0) => ({
  key: label, label, costUsd, calls, errors, cacheHit: 0.85, hasPrice: true, sessions, overheadTokens: sessions ? 10_600 + sessions * 20 : 0,
});

const HOUR = 3_600_000;
const sessionRows = [
  ["s1", "claude-code", "Claude Code", "AgentBoard", "feat/v2-sesiones", "claude-opus-4-5", 0, 6.9, 38.62, 312, 24, 4, 486, 11],
  ["s2", "claude-code", "Claude Code", "tuio-web", "fix/pagos", "claude-opus-4-5", 1.1, 1.86, 27.35, 240, 18, 3, 301, 6],
  ["s3", "codex", "Codex CLI", "tuio-web", "main", "gpt-5-codex", 1.5, 0.8, 6.8, 90, 0, 0, 70, 3],
  ["s4", "claude-code", "Claude Code", "AgentBoard", "main", "claude-sonnet-5", 2.3, 1.16, 9.44, 150, 15, 1, 180, 2],
  ["s5", "opencode", "OpenCode", "scripts", "main", "claude-sonnet-5", 3.1, 0.37, 2.15, 40, 6, 0, 35, 0],
  ["s6", "claude-code", "Claude Code", "infra", "chore/modulos", "claude-opus-4-5", 4.2, 3.07, 24.9, 280, 31, 4, 410, 9],
  ["s7", "claude-code", "Claude Code", "infra", "main", "claude-haiku-4-5", 6.4, 0.23, 0.38, 20, 4, 0, 18, 0],
  ["s8", "opencode", "OpenCode", "tuio-web", "fix/timeouts", "modelo-local", 9.0, 0.66, 0, 60, 10, 0, 50, 1],
] as const;
const sessions = [
  { id: "live-codex", agentId: "codex", agentName: "Codex CLI", projectId: 1, project: "AgentBoard", branch: "feat/v2", startedAt: now - 60_000, endedAt: now - 20_000, model: "gpt-5-codex", costUsd: 0.12, calls: 3, cacheHit: 0.8, inputTokens: 90_000, outputTokens: 900, hasPrice: true, turns: 1, compactions: 0, toolCalls: 2, toolErrors: 0, subagentCalls: 0, isSubagent: false, projectKey: "/AgentBoard" },
  ...sessionRows.map(([id, agentId, agentName, project, branch, model, daysAgo, hours, costUsd, calls, turns, compactions, toolCalls, toolErrors], i) => {
  const startedAt = now - daysAgo * DAY - hours * HOUR;
  return {
    id, agentId, agentName, projectId: i + 1, project, branch, startedAt, endedAt: startedAt + hours * HOUR, model, costUsd, calls,
    cacheHit: 0.9 + (i % 4) * 0.02, inputTokens: calls * 90_000, outputTokens: calls * 900, hasPrice: model !== "modelo-local",
    turns, compactions, toolCalls, toolErrors, subagentCalls: i % 3 === 0 ? 12 : 0, isSubagent: false, projectKey: `/${project}`,
  };
}),
];

function mockProjects() {
  const by = new Map<string, typeof sessions>();
  for (const x of sessions) by.set(x.project, [...(by.get(x.project) ?? []), x]);
  return [...by.entries()].map(([name, ss], i) => ({
    key: `/${name}`, projectId: i + 1, name, path: `/home/u/Proyectos/${name}`,
    agents: [...new Map(ss.map((x) => [x.agentId, { id: x.agentId, name: x.agentName }])).values()],
    branches: [...new Set(ss.map((x) => x.branch))], model: ss[0].model, sessions: ss.length,
    activeMs: ss.reduce((a, x) => a + x.endedAt - x.startedAt, 0), turns: ss.reduce((a, x) => a + x.turns, 0),
    compactions: ss.reduce((a, x) => a + x.compactions, 0), toolCalls: ss.reduce((a, x) => a + x.toolCalls, 0),
    toolErrors: ss.reduce((a, x) => a + x.toolErrors, 0), subagentCalls: ss.reduce((a, x) => a + x.subagentCalls, 0),
    costUsd: ss.reduce((a, x) => a + x.costUsd, 0), calls: ss.reduce((a, x) => a + x.calls, 0), cacheHit: 0.95,
    hasPrice: ss.every((x) => x.hasPrice), firstTs: Math.min(...ss.map((x) => x.startedAt)), lastTs: Math.max(...ss.map((x) => x.endedAt)),
  })).sort((a, b) => b.costUsd - a.costUsd);
}

function sessionDetail(id: string) {
  const s = sessions.find((x) => x.id === id) ?? sessions[0];
  const n = Math.max(s.turns, 1);
  const weights = Array.from({ length: n }, (_, i) => 1 + i * 0.12 + ((i * 7) % 5) * 0.2);
  const total = weights.reduce((a, b) => a + b, 0);
  const span = s.endedAt - s.startedAt;
  const acts = ["feature", "coding", "testing", "debugging", "exploration"];
  const turns = weights.map((w, i) => ({
    n: i + 1, id: `${id}-t${i}`, ts: s.startedAt + (span * i) / n, activity: acts[(i * 3) % acts.length],
    costUsd: (s.costUsd * w) / total, calls: Math.round(s.calls / n), inputTokens: 400_000 + i * 90_000, outputTokens: 6000 + i * 700,
    tools: [["Edit", 3 + (i % 4)], ["Bash", 1 + (i % 3)], ["Read", 2]] as [string, number][], toolErrors: i % 5 === 2 ? 1 : 0,
  }));
  let acc = 0;
  const timeline = turns.map((t) => ({ ts: t.ts, costUsd: (acc += t.costUsd) }));
  const compactions = Array.from({ length: s.compactions }, (_, i) => s.startedAt + (span * (i + 1.4)) / (s.compactions + 1));
  const byAct = new Map<string, number>();
  for (const t of turns) byAct.set(t.activity, (byAct.get(t.activity) ?? 0) + t.costUsd);
  return {
    session: s, timeline, compactions, turns: s.turns ? turns : [],
    activities: [...byAct.entries()].map(([key, costUsd]) => ({ key, costUsd, calls: 10 })).sort((a, b) => b.costUsd - a.costUsd),
    models: [{ key: s.model, costUsd: s.costUsd * 0.9, calls: s.calls - 20 }, { key: "claude-haiku-4-5", costUsd: s.costUsd * 0.1, calls: 20 }],
    tools: [
      { tool: "Edit", calls: 142, errors: 0, p50Ms: 110, p95Ms: 420 },
      { tool: "Read", calls: 118, errors: 0, p50Ms: 90, p95Ms: 300 },
      { tool: "Bash", calls: 96, errors: 9, p50Ms: 2100, p95Ms: 38_000 },
      { tool: "Grep", calls: 64, errors: 0, p50Ms: 200, p95Ms: 900 },
      { tool: "mcp__agentboard__get_summary", calls: 12, errors: 0, p50Ms: 180, p95Ms: 420 },
      { tool: "Task", calls: 6, errors: 0, p50Ms: 48_000, p95Ms: 130_000 },
      { tool: "WebFetch", calls: 3, errors: 1, p50Ms: null, p95Ms: null },
    ],
  };
}

function mockSettings() {
  return {
    theme: "system", language: "system", monthlyBudget: 60, dailyBudget: 5, alertAt80: true, alertAt100: true, trayShowsToday: false, memoryOnly: false,
    budgets: [{ kind: "agent", key: "codex", label: "Codex CLI", monthly: 5 }, { kind: "project", key: "AgentBoard", label: "AgentBoard", monthly: 40 }],
    priceOverrides: [{ model: "claude-haiku-4-5", input: 0.8, output: 4, cacheRead: 0.08, cacheWrite: 1, cacheWrite1h: 1.6 }],
    ...JSON.parse(localStorage.getItem("mockSettings") ?? "{}"),
  };
}

export function installMocks() {
  const handle = (cmd: string, args: unknown): unknown => {
    const a = args as Record<string, unknown>;
    // El Resumen en una petición: los mismos apartados que los comandos sueltos.
    if (cmd === "get_dashboard_snapshot") {
      const r = a.request as { filter: object; tz: string; monthFrom: number; branchView: boolean; previous: object | null };
      const q = (c: string, extra: Record<string, unknown> = {}, filter: object = r.filter) => handle(c, { filter, tz: r.tz, ...extra });
      const by = (k: string) => q("get_breakdown", { by: k });
      const project = r.branchView ? "branch" : "project";
      return {
        summary: q("get_summary"),
        agents: by("agent"),
        daily: q("get_timeseries", { bucket: "day" }),
        hourly: q("get_timeseries", { bucket: "hour" }),
        dailyByAgent: q("get_timeseries_by", { by: "agent" }),
        dailyByModel: q("get_timeseries_by", { by: "model" }),
        dailyByProject: q("get_timeseries_by", { by: project }),
        dailyByTool: q("get_timeseries_by", { by: "tool" }),
        month: q("get_timeseries", { bucket: "day" }, { ...r.filter, from: r.monthFrom, to: undefined }),
        projects: by("project"),
        branches: r.branchView ? by("branch") : null,
        models: by("model"),
        activity: q("get_activity"),
        activityDaily: q("get_activity_daily"),
        tools: by("tool"),
        commands: by("command"),
        skills: by("skill"),
        mcp: by("mcp"),
        agentTypes: by("agent_type"),
        prev: r.previous
          ? {
              summary: q("get_summary", {}, r.previous),
              daily: q("get_timeseries", { bucket: "day" }, r.previous),
              projects: q("get_breakdown", { by: project }, r.previous),
            }
          : null,
      };
    }
    // Un filtro con `to` es el periodo anterior (comparación): cifras algo distintas.
    const f = (a.filter ?? {}) as { from?: number; to?: number };
    if (f.to != null) {
      if (cmd === "get_summary")
        return {
          costUsd: 47.4, calls: 3310, sessions: 58, inputTokens: 19000, outputTokens: 1_210_000,
          cacheRead: 160_000_000, cacheWrite: 7_100_000, cacheHit: 0.962, cacheSavingsUsd: 501.2,
          burnRateUsdH: 0, firstTs: f.from ?? null, lastTs: f.to, unpricedModels: [],
        };
      if (cmd === "get_timeseries" && a.bucket === "day") {
        const out = [];
        for (let ts = f.from ?? f.to - 30 * DAY, i = 0; ts < f.to; ts += DAY, i++)
          if (i % 4 !== 2) out.push({ ts, costUsd: 0.8 + ((i * 29) % 9) * 0.45, calls: 90, sessions: 2, inputTokens: 0, outputTokens: 0, cacheRead: 0, cacheWrite: 0 });
        return out;
      }
      if (cmd === "get_breakdown" && (a.by === "project" || a.by === "branch"))
        return [{ ...row("AgentBoard", 17.0, 700, 0, 10), key: "/w" }, { ...row("tuio-web", 19.9, 1200, 0, 9), key: "/t" }, { ...row("infra", 9.0, 650, 0, 4), key: "/i" }, row("main", 10, 400, 0, 4)];
    }
    switch (cmd) {
      case "get_summary":
        return {
          costUsd: 56.0, calls: 3792, sessions: 64, inputTokens: 21000, outputTokens: 1_450_000,
          cacheRead: 182_000_000, cacheWrite: 9_400_000, cacheHit: 0.95, cacheSavingsUsd: 612.4,
          burnRateUsdH: 2.35, firstTs: now - 42 * DAY, lastTs: now, unpricedModels: ["modelo-local"],
        };
      case "get_timeseries": {
        const pt = (ts: number, costUsd: number, calls: number) => ({
          ts, costUsd, calls, sessions: Math.max(1, Math.round(calls / 40)),
          inputTokens: calls * 120, outputTokens: calls * 350, cacheRead: calls * 9000, cacheWrite: calls * 400,
        });
        if (a.bucket === "hour") {
          const out = [];
          for (let d = 0; d < 14; d++) for (const h of [9, 10, 11, 12, 15, 16, 17, 18, 22]) {
            const ts = new Date(new Date(now - d * DAY).setHours(h, 0, 0, 0)).getTime();
            out.push(pt(ts, 0.2 + ((d * h) % 7) * 0.15, 10 + ((d + h) % 9)));
          }
          return out;
        }
        // Solo algunos días tienen actividad, como en la realidad.
        const days = Math.floor((now - monthStart) / DAY) + 1;
        return Array.from({ length: days }, (_, i) => pt(monthStart + i * DAY, 1 + ((i * 37) % 11) * 0.6, 100 + (i % 5) * 20)).filter((_, i) => i % 3 !== 1);
      }
      case "get_timeseries_by": {
        const keys: Record<string, [string, string][]> = {
          // Gemini con coste 0 (instalado, sin precio): como en una máquina real.
          agent: [["claude-code", "Claude Code"], ["codex", "Codex CLI"], ["opencode", "OpenCode"], ["gemini", "Gemini CLI"]],
          model: [["claude-fable-5-1", "claude-fable-5-1"], ["claude-opus-5-5", "claude-opus-5-5"], ["claude-sonnet-5", "claude-sonnet-5"], ["gpt-5-codex", "gpt-5-codex"]],
          project: [["/w", "AgentBoard"], ["/t", "tuio-web"], ["/i", "infra"], ["/s", "scripts"]],
          branch: [["main", "main"], ["feat/dashboard", "feat/dashboard"]],
          tool: [["Bash", "Bash"], ["Read", "Read"], ["Edit", "Edit"], ["Grep", "Grep"], ["Write", "Write"]],
        };
        const out: { ts: number; key: string; label: string; costUsd: number; calls: number; outputTokens: number }[] = [];
        for (let d = 13; d >= 0; d--) {
          const ts = Math.floor((now - d * DAY) / DAY) * DAY;
          (keys[String(a.by)] ?? []).forEach(([key, label], i) => {
            if ((d + i) % 4 === 3) return;
            const w = 1 / (i + 1);
            out.push({ ts, key, label, costUsd: a.by === "tool" || key === "gemini" ? 0 : (1 + ((d * 37) % 11) * 0.5) * w, calls: Math.round(80 * w) + (d % 3) * 5, outputTokens: Math.round(30000 * w) });
          });
        }
        return out;
      }
      case "get_breakdown":
        switch (a.by) {
          case "agent": return [row("Claude Code", 51.9, 3460, 0, 58).key === "Claude Code" ? { ...row("Claude Code", 51.9, 3460, 0, 58), key: "claude-code" } : row("x", 0, 0), { ...row("Codex CLI", 4.1, 332, 0, 6), key: "codex" }, { ...row("OpenCode", 0, 84, 0, 3), key: "opencode" }];
          case "model": return models;
          // Clave = raíz del repo, como en el núcleo (y en la serie diaria por proyecto).
          case "project": return [{ ...row("AgentBoard", 22.1, 900, 0, 12), key: "/w" }, { ...row("tuio-web", 18.4, 1300, 0, 9), key: "/t" }, { ...row("infra", 9.2, 700, 0, 4), key: "/i" }, { ...row("scripts", 6.3, 890, 0, 3), key: "/s" }];
          case "branch": return [row("main", 12.0, 500, 0, 5), row("feat/dashboard", 8.1, 300, 0, 3), row("fix/ingesta", 2.0, 100, 0, 1)];
          // Importes pequeños (suman < $1), como en una máquina real.
          case "skill": return [row("claude-in-chrome", 0.15, 2), row("claude-api", 0.07, 1), row("dataviz", 0.05, 1), row("anthropic-skills:docs", 0.02, 1)];
          case "mcp": return [row("figma", 0, 23), row("claude_ai_Slack", 0, 5), row("claude_ai_Supabase", 0, 1)];
          case "agent_type": return [
            { ...row("general-purpose", 100.4, 1652), key: "claude-code:general-purpose", agent: "claude-code" },
            { ...row("general", 12.3, 210), key: "opencode:general", agent: "opencode" },
            { ...row("code-reviewer", 9.21, 104), key: "claude-code:code-reviewer", agent: "claude-code" },
            { ...row("worker", 6.1, 95), key: "codex:worker", agent: "codex" },
            { ...row("Explore", 4.8, 82), key: "claude-code:Explore", agent: "claude-code" },
          ];
          case "tool": return [row("Bash", 0, 1204, 96), row("Read", 0, 980, 4), row("Edit", 0, 702, 21), row("Grep", 0, 410, 0), row("Write", 0, 120, 2)].map((r) => ({ ...r, costUsd: 0 }));
          case "command": return [row("git", 0, 402, 3), row("cargo", 0, 310, 44), row("npm", 0, 280, 20), row("ls", 0, 150, 0), row("rg", 0, 62, 1)].map((r) => ({ ...r, costUsd: 0 }));
        }
        return [];
      case "get_activity":
        return {
          activities: [
            { key: "coding", costUsd: 24.3, turns: 268, editTurns: 250, oneShot: 0.96 },
            { key: "exploration", costUsd: 14.2, turns: 31, editTurns: 0, oneShot: null },
            { key: "testing", costUsd: 9.9, turns: 30, editTurns: 0, oneShot: null },
            { key: "delegation", costUsd: 2.6, turns: 9, editTurns: 0, oneShot: null },
            { key: "conversation", costUsd: 1.2, turns: 90, editTurns: 0, oneShot: null },
            { key: "build", costUsd: 1.1, turns: 7, editTurns: 0, oneShot: null },
            { key: "feature", costUsd: 0.9, turns: 4, editTurns: 4, oneShot: 1 },
            { key: "debugging", costUsd: 0.8, turns: 6, editTurns: 6, oneShot: 0.83 },
            { key: "brainstorming", costUsd: 0.6, turns: 6, editTurns: 0, oneShot: null },
            { key: "other", costUsd: 0.4, turns: 9, editTurns: 0, oneShot: null },
          ],
          models: [{ model: "claude-opus-5-5", editTurns: 200, oneShot: 0.92 }, { model: "claude-fable-5-1", editTurns: 60, oneShot: 1 }],
        };
      case "get_activity_daily": {
        const out = [];
        for (let i = 13; i >= 0; i--) {
          const ts = Math.floor((now - i * DAY) / DAY) * DAY;
          out.push({ ts, activity: "coding", costUsd: 1 + ((i * 7) % 5) * 0.5, turns: 10 });
          out.push({ ts, activity: "exploration", costUsd: 0.4 + ((i * 3) % 4) * 0.3, turns: 3 });
          out.push({ ts, activity: "testing", costUsd: 0.3 + (i % 3) * 0.3, turns: 2 });
          out.push({ ts, activity: "conversation", costUsd: 0.1, turns: 4 });
        }
        return out;
      }
      case "list_agents":
        return [
          { id: "claude-code", name: "Claude Code", logRoot: "~/.claude/projects", costUsd: 51.9, calls: 3460 },
          { id: "codex", name: "Codex CLI", logRoot: "~/.codex/sessions", costUsd: 4.1, calls: 332 },
        ];
      case "list_projects":
        return [
          { id: 1, name: "AgentBoard", cwd: "/home/u/Proyectos/AgentBoard", costUsd: 22.1, calls: 900 },
          { id: 2, name: "tuio-web", cwd: "/home/u/Proyectos/tuio-web", costUsd: 18.4, calls: 1300 },
          { id: 3, name: "infra", cwd: "/home/u/infra", costUsd: 9.2, calls: 700 },
          { id: 4, name: "scripts", cwd: "/home/u/scripts", costUsd: 6.3, calls: 890 },
        ];
      case "list_prices": {
        const overrides = new Map((mockSettings().priceOverrides as { model: string; input: number; output: number; cacheRead: number; cacheWrite: number; cacheWrite1h: number }[]).map((p) => [p.model, p]));
        const base: [string, number[] | null, number][] = [
          ["modelo-local", null, 12], ["kimi-k2", null, 148], ["claude-fable-5-1", [10, 50, 0.25, 12.5, 20], 742], ["claude-opus-5-5", [4, 20, 0.2, 5, 8], 1840],
          ["claude-sonnet-5", [2, 10, 0.2, 2.5, 4], 1210], ["claude-haiku-4-5", [1, 5, 0.1, 1.25, 2], 410], ["gpt-5-codex", [1.25, 10, 0.125, 0, 0], 320],
          ["big-pickle", null, 40], ["gemini-3-pro", [2, 12, 0.2, 0, 0], 0], ["o3", [2, 8, 0.5, 0, 0], 0],
        ];
        return base.map(([model, prices, calls]) => {
          const o = overrides.get(model);
          const p = o ? [o.input, o.output, o.cacheRead, o.cacheWrite, o.cacheWrite1h] : prices;
          const source = o ? "edited" : prices ? "default" : model === "big-pickle" ? "reported" : "missing";
          return { model, prices: p, source, calls, costUsd: p ? calls * 0.01 : 0 };
        }).sort((a, b) => Number(b.source === "missing") - Number(a.source === "missing") || b.calls - a.calls);
      }
      case "list_project_summaries":
        return mockProjects();
      case "get_project_detail": {
        const proj = mockProjects().find((x) => x.key === a.key) ?? mockProjects()[0];
        const ses = sessions.filter((x) => `/${x.project}` === proj.key);
        const detail = sessionDetail(ses[0]?.id ?? "s1");
        const days = Math.floor((now - monthStart) / DAY) + 1;
        return {
          project: proj,
          daily: Array.from({ length: days }, (_, i) => ({ ts: monthStart + i * DAY, costUsd: (proj.costUsd / days) * (0.4 + ((i * 7) % 5) * 0.3), calls: 20, sessions: 1, inputTokens: 0, outputTokens: 0, cacheRead: 0, cacheWrite: 0 })).filter((_, i) => i % 3 !== 1),
          activities: [{ key: "feature", costUsd: proj.costUsd * 0.4, turns: 20, editTurns: 18, oneShot: 0.9 }, { key: "coding", costUsd: proj.costUsd * 0.3, turns: 30, editTurns: 30, oneShot: 0.95 }, { key: "testing", costUsd: proj.costUsd * 0.2, turns: 8, editTurns: 0, oneShot: null }, { key: "exploration", costUsd: proj.costUsd * 0.1, turns: 6, editTurns: 0, oneShot: null }],
          models: detail.models.map((m) => ({ ...row(m.key, m.costUsd, m.calls), key: m.key })),
          branches: [...new Set(ses.map((x) => x.branch))].map((b, i) => row(b, proj.costUsd / (i + 2), 100 - i * 20)),
          sessions: ses,
          tools: detail.tools,
        };
      }
      case "list_sessions":
        return { sessions, total: sessions.length };
      case "get_session_detail":
        return sessionDetail(String(a.id));
      case "get_data_info":
        return { firstTs: now - 42 * DAY, calls: 3792, watchedFiles: 212, lastScan: now - 60_000 };
      case "export_data":
        return a.format === "json" ? "[]" : "ts,iso,agent,project,branch,model,input_tokens,output_tokens,cost_usd\n";
      case "get_settings":
        return mockSettings();
      case "set_settings":
        localStorage.setItem("mockSettings", JSON.stringify(a.settings));
        return a.settings;
      case "rebuild_cache":
        // Como el backend: avisa de que pudo cambiar todo y la relectura termina un poco
        // después con `ingest://done`.
        setTimeout(() => void emit("data:changed", { generation: ++generation, agents: [], days: [] }), 400);
        setTimeout(() => void emit("ingest://done", { files: 212, lines: 0, records: 0, errors: 0, full: true }), 800);
        return null;
    }
    console.warn("mock sin implementar:", cmd, args);
    return null;
  };
  mockIPC(handle, { shouldMockEvents: true });
  // Relecturas simuladas del vigilante: un aviso de datos cambiados de vez en cuando.
  setInterval(() => {
    const d = new Date();
    const day = d.getFullYear() * 10_000 + (d.getMonth() + 1) * 100 + d.getDate();
    void emit("data:changed", { generation: ++generation, agents: ["claude-code"], days: [day] });
  }, 20_000);
}

/** Generación de datos simulada de `data:changed`. */
let generation = 0;

/**
 * Sonda de maquetación (solo dev, con `?probe=1`): busca elementos que desbordan en
 * horizontal sin ser scroll a propósito y pinta una chapa con el resultado, visible
 * también en las capturas. Verde = nada roto; rojo = lista de clases culpables.
 */
export function installOverflowProbe() {
  // Los errores también delatan una vista rota en las capturas.
  window.addEventListener("error", (e) => console.log("[probe-error]", String(e.message)));
  window.addEventListener("unhandledrejection", (e) => console.log("[probe-reject]", String(e.reason)));
  const badge = document.createElement("div");
  badge.id = "probe-badge";
  badge.style.cssText =
    "position:fixed;left:8px;bottom:8px;z-index:9999;font:11px monospace;padding:4px 8px;border-radius:6px;color:#fff;max-width:70vw;pointer-events:none;white-space:pre-wrap";
  document.body.appendChild(badge);
  const scrollable = (el: Element | null): boolean => {
    for (; el && el !== document.body; el = el.parentElement) {
      const o = getComputedStyle(el).overflowX;
      if (o === "auto" || o === "scroll") return true;
    }
    return false;
  };
  const mark = () => {
    const vw = document.documentElement.clientWidth;
    const bad = new Set<string>();
    const name = (el: Element) => (typeof el.className === "string" && el.className ? el.className.split(" ")[0] : el.tagName.toLowerCase());
    document.querySelectorAll("body *").forEach((el) => {
      if (el.id === "probe-badge") return;
      const cs = getComputedStyle(el);
      if (cs.display === "none" || cs.position === "fixed") return;
      // Contenido recortado de verdad (overflow hidden/clip) sin puntos suspensivos: datos perdidos.
      if (el.scrollWidth - el.clientWidth > 2 && /(hidden|clip)/.test(cs.overflowX) && cs.textOverflow !== "ellipsis") bad.add(name(el));
      // Se sale del ancho de la ventana (y no está dentro de un scroll a propósito).
      const r = el.getBoundingClientRect();
      if (r.right > vw + 2 && r.width > 0 && !scrollable(el)) bad.add("↦" + name(el));
    });
    const list = [...bad].slice(0, 6).join(" · ");
    badge.textContent = bad.size ? `DESBORDA (${bad.size}): ${list}` : `OK ${vw}px`;
    badge.style.background = bad.size ? "#c43434" : "#16813f";
  };
  // Marca enseguida y a menudo: las capturas headless pueden llegar muy pronto.
  mark();
  setInterval(mark, 200);
}
