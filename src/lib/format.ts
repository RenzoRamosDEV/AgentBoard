// Cifras al estilo terminal de CodeBurn ($1,234.56 · 21.3K · 97.6%); los textos siguen en español.
const usd2 = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD", minimumFractionDigits: 2, maximumFractionDigits: 2 });
const usd4 = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD", maximumFractionDigits: 4 });
const int = new Intl.NumberFormat("en-US");
const compact = new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 });
const pct = new Intl.NumberFormat("en-US", { style: "percent", maximumFractionDigits: 1 });

export const fmt = {
  usd: (n: number) => (n !== 0 && Math.abs(n) < 0.01 ? usd4 : usd2).format(n),
  int: (n: number) => int.format(n),
  compact: (n: number) => compact.format(n),
  pct: (n: number) => pct.format(n),
  date: (ts: number) => new Date(ts).toLocaleDateString("es-ES", { day: "numeric", month: "short", year: "numeric" }),
  day: (ts: number) => new Date(ts).toLocaleDateString("es-ES", { day: "numeric", month: "short" }),
  bytes: (n: number) => {
    const units = ["B", "KB", "MB", "GB"];
    let i = 0;
    while (n >= 1024 && i < units.length - 1) {
      n /= 1024;
      i++;
    }
    return `${n.toFixed(i ? 1 : 0)} ${units[i]}`;
  },
};

/** Etiqueta y color (token CSS) de cada actividad. */
export const ACTIVITIES: Record<string, { label: string; color: string }> = {
  coding: { label: "Coding", color: "var(--act-coding)" },
  feature: { label: "Feature Dev", color: "var(--act-feature)" },
  debugging: { label: "Debugging", color: "var(--act-debug)" },
  testing: { label: "Testing", color: "var(--act-testing)" },
  build: { label: "Build/Deploy", color: "var(--act-build)" },
  git: { label: "Git", color: "var(--act-git)" },
  exploration: { label: "Exploration", color: "var(--act-explore)" },
  delegation: { label: "Delegation", color: "var(--act-delegation)" },
  brainstorming: { label: "Brainstorming", color: "var(--act-brainstorm)" },
  conversation: { label: "Conversation", color: "var(--text-muted)" },
  shell: { label: "Shell", color: "var(--act-build)" },
  research: { label: "Research", color: "var(--act-explore)" },
  other: { label: "General", color: "var(--text-muted)" },
};

/** `claude-opus-5-5` → `Opus 5.5`; otros modelos tal cual. */
export function modelName(id: string): string {
  const m = id.match(/^claude-(?:(\d+)-(\d+)-)?([a-z]+)(?:-(\d+))?(?:-(\d+))?$/);
  if (!m) return id;
  const [, oldMajor, oldMinor, family, major, minor] = m;
  const name = family.charAt(0).toUpperCase() + family.slice(1);
  if (oldMajor) return `${name} ${oldMajor}.${oldMinor}`;
  return [name, [major, minor].filter(Boolean).join(".")].filter(Boolean).join(" ");
}
