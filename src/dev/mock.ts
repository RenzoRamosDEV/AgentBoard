/**
 * Datos de ejemplo para desarrollar la UI en un navegador normal (`npm run dev`),
 * sin el núcleo Rust. Solo se carga en modo dev y fuera de Tauri.
 */
import { mockIPC } from "@tauri-apps/api/mocks";

const DAY = 864e5;
const now = Date.now();
const monthStart = new Date(new Date().getFullYear(), new Date().getMonth(), 1).getTime();

const models = [
  { key: "claude-opus-5-5", label: "claude-opus-5-5", costUsd: 41.2, calls: 1840, errors: 0, cacheHit: 0.91, hasPrice: true },
  { key: "claude-sonnet-5", label: "claude-sonnet-5", costUsd: 9.8, calls: 1210, errors: 0, cacheHit: 0.87, hasPrice: true },
  { key: "gpt-5-codex", label: "gpt-5-codex", costUsd: 4.1, calls: 320, errors: 0, cacheHit: 0.62, hasPrice: true },
  { key: "claude-haiku-4-5", label: "claude-haiku-4-5", costUsd: 0.9, calls: 410, errors: 0, cacheHit: 0.8, hasPrice: true },
  { key: "modelo-local", label: "modelo-local", costUsd: 0, calls: 12, errors: 0, cacheHit: 0, hasPrice: false },
];
const row = (label: string, costUsd: number, calls: number, errors = 0) => ({ key: label, label, costUsd, calls, errors, cacheHit: 0.85, hasPrice: true });

export function installMocks() {
  mockIPC((cmd, args) => {
    const a = args as Record<string, unknown>;
    switch (cmd) {
      case "get_summary":
        return {
          costUsd: 56.0, calls: 3792, sessions: 64, inputTokens: 21000, outputTokens: 1_450_000,
          cacheRead: 182_000_000, cacheWrite: 9_400_000, cacheHit: 0.95, cacheSavingsUsd: 612.4,
          burnRateUsdH: 2.35, firstTs: now - 42 * DAY, lastTs: now, unpricedModels: ["modelo-local"],
        };
      case "get_timeseries": {
        const days = Math.floor((now - monthStart) / DAY) + 1;
        return Array.from({ length: days }, (_, i) => ({ ts: monthStart + i * DAY, costUsd: 1 + ((i * 37) % 11) * 0.6, calls: 100 }));
      }
      case "get_breakdown":
        switch (a.by) {
          case "model": return models;
          case "project": return [row("AgentBoard", 22.1, 900), row("tuio-web", 18.4, 1300), row("infra", 9.2, 700), row("scripts", 6.3, 890)];
          case "branch": return [row("main", 12.0, 500), row("feat/dashboard", 8.1, 300), row("fix/ingesta", 2.0, 100)];
          case "activity": return [row("coding", 24.3, 1500), row("exploration", 14.2, 1100), row("testing", 9.9, 600), row("shell", 5.1, 400), row("conversation", 2.5, 192)];
          case "tool": return [row("Bash", 0, 1204, 96), row("Read", 0, 980, 4), row("Edit", 0, 702, 21), row("Grep", 0, 410, 0), row("Write", 0, 120, 2)].map((r) => ({ ...r, costUsd: 0 }));
          case "command": return [row("git", 0, 402, 3), row("cargo", 0, 310, 44), row("npm", 0, 280, 20), row("ls", 0, 150, 0), row("rg", 0, 62, 1)].map((r) => ({ ...r, costUsd: 0 }));
        }
        return [];
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
      case "get_data_info":
        return { firstTs: now - 42 * DAY, dbBytes: 18_400_000, watchedFiles: 212, dbPath: "~/.local/share/agentburn/agentburn.db" };
      case "get_settings":
        return { monthlyBudget: 60 };
      case "set_settings":
        return a.settings;
      case "plugin:event|listen":
        return 1;
      case "plugin:event|unlisten":
        return null;
    }
    console.warn("mock sin implementar:", cmd, args);
    return null;
  });
}
