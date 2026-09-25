const usd2 = new Intl.NumberFormat("es-ES", { style: "currency", currency: "USD", maximumFractionDigits: 2 });
const usd4 = new Intl.NumberFormat("es-ES", { style: "currency", currency: "USD", maximumFractionDigits: 4 });
const int = new Intl.NumberFormat("es-ES");
const compact = new Intl.NumberFormat("es-ES", { notation: "compact", maximumFractionDigits: 1 });
const pct = new Intl.NumberFormat("es-ES", { style: "percent", maximumFractionDigits: 1 });

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

export const ACTIVITY_LABELS: Record<string, string> = {
  coding: "Código",
  testing: "Tests",
  shell: "Shell",
  exploration: "Exploración",
  research: "Investigación",
  delegation: "Subagentes",
  conversation: "Conversación",
  other: "Otras",
};
