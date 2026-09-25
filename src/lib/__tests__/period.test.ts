import { describe, expect, it } from "vitest";
import { periodRange, projectMonth, PERIODS } from "../period";

describe("periodRange", () => {
  const now = new Date(2026, 8, 25, 15, 0, 0); // 25 sep 2026, hora local

  it("cubre los últimos N días incluido hoy", () => {
    const r = periodRange({ kind: "7d" }, now);
    // desde el día 19 a las 00:00 locales
    expect(new Date(r.from!)).toEqual(new Date(2026, 8, 19, 0, 0, 0));
    expect(r.to).toBeUndefined();
  });

  it('"Todo" no pone límites', () => {
    expect(periodRange({ kind: "all" }, now)).toEqual({});
  });

  it("30 días arranca 29 días atrás a medianoche", () => {
    const r = periodRange({ kind: "30d" }, now);
    expect(new Date(r.from!)).toEqual(new Date(2026, 7, 27, 0, 0, 0));
  });

  it("solo ofrece 7/30/60/90/todo", () => {
    expect(PERIODS.map((p) => p.kind)).toEqual(["7d", "30d", "60d", "90d", "all"]);
  });
});

describe("projectMonth", () => {
  it("proyecta linealmente: 20 el día 10 de un mes de 30 → 60", () => {
    const day10 = new Date(2026, 8, 10, 12, 0, 0); // septiembre tiene 30 días
    expect(projectMonth(20, day10)).toBeCloseTo(60, 5);
  });
});
