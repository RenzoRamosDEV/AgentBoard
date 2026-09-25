/** Periodos del filtro, calculados en hora local. */
export type PeriodKind = "7d" | "30d" | "60d" | "90d" | "all";

export interface Period {
  kind: PeriodKind;
}

export const PERIODS: { kind: PeriodKind; label: string; days?: number }[] = [
  { kind: "7d", label: "7 días", days: 7 },
  { kind: "30d", label: "30 días", days: 30 },
  { kind: "60d", label: "60 días", days: 60 },
  { kind: "90d", label: "90 días", days: 90 },
  { kind: "all", label: "Todo" },
];

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
const addDays = (d: Date, n: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
/** Rango `[from, to)` en epoch ms: los últimos N días incluido hoy; `undefined` = sin límite. */
export function periodRange(p: Period, now = new Date()): { from?: number; to?: number } {
  const days = PERIODS.find((x) => x.kind === p.kind)?.days;
  if (!days) return {};
  return { from: addDays(startOfDay(now), -(days - 1)).getTime() };
}

export const monthStart = (now = new Date()) => new Date(now.getFullYear(), now.getMonth(), 1);
export const daysInMonth = (now = new Date()) => new Date(now.getFullYear(), now.getMonth() + 1, 0).getDate();

/** Proyección lineal a fin de mes: acumulado / días transcurridos × días del mes. */
export function projectMonth(spent: number, now = new Date()): number {
  const elapsed = now.getDate();
  return (spent / elapsed) * daysInMonth(now);
}
