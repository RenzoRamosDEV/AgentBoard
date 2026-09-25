/** Periodos del filtro, calculados en hora local. */
export type PeriodKind = "today" | "7d" | "30d" | "month" | "6m" | "all" | "custom";

export interface Period {
  kind: PeriodKind;
  /** Solo para "custom": fechas locales `YYYY-MM-DD`, ambas incluidas. */
  start?: string;
  end?: string;
}

export const PERIODS: { kind: PeriodKind; label: string }[] = [
  { kind: "today", label: "Hoy" },
  { kind: "7d", label: "7 días" },
  { kind: "30d", label: "30 días" },
  { kind: "month", label: "Mes" },
  { kind: "6m", label: "6 meses" },
  { kind: "all", label: "Todo" },
  { kind: "custom", label: "Rango" },
];

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
const addDays = (d: Date, n: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
const parseLocal = (s: string) => {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
};

/** Rango `[from, to)` en epoch ms; `undefined` = sin límite. */
export function periodRange(p: Period, now = new Date()): { from?: number; to?: number } {
  const today = startOfDay(now);
  switch (p.kind) {
    case "today":
      return { from: today.getTime() };
    case "7d":
      return { from: addDays(today, -6).getTime() };
    case "30d":
      return { from: addDays(today, -29).getTime() };
    case "month":
      return { from: new Date(now.getFullYear(), now.getMonth(), 1).getTime() };
    case "6m":
      return { from: new Date(now.getFullYear(), now.getMonth() - 6, now.getDate()).getTime() };
    case "all":
      return {};
    case "custom": {
      if (!p.start || !p.end) return {};
      const [a, b] = [p.start, p.end].sort();
      return { from: parseLocal(a).getTime(), to: addDays(parseLocal(b), 1).getTime() };
    }
  }
}

export const monthStart = (now = new Date()) => new Date(now.getFullYear(), now.getMonth(), 1);
export const daysInMonth = (now = new Date()) => new Date(now.getFullYear(), now.getMonth() + 1, 0).getDate();

/** Proyección lineal a fin de mes: acumulado / días transcurridos × días del mes. */
export function projectMonth(spent: number, now = new Date()): number {
  const elapsed = now.getDate();
  return (spent / elapsed) * daysInMonth(now);
}

export const toInputDate = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
