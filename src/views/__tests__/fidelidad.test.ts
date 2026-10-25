import { describe, expect, it } from "vitest";
import { parsePriceFile } from "../Pricing";
import { filterItems } from "../../components/CommandPalette";


describe("parsePriceFile", () => {
  it("acepta lista u objeto", () => {
    expect(parsePriceFile('[{"model":"Kimi-K2","input":1,"output":4}]')).toEqual({ "kimi-k2": ["1", "4", "", "", ""] });
    expect(parsePriceFile('{"x":{"input":0.5,"output":2,"cacheRead":0.05}}')).toEqual({ x: ["0.5", "2", "0.05", "", ""] });
  });
  it("rechaza precios inválidos", () => {
    expect(() => parsePriceFile('[{"model":"x","input":-1}]')).toThrow();
  });
});

describe("filterItems", () => {
  it("todas las palabras deben aparecer", () => {
    const items = [
      { id: "1", group: "Proyectos", label: "tuio-api", run: () => {} },
      { id: "2", group: "Apartados", label: "Sesiones", hint: "¿Qué sesión se disparó?", run: () => {} },
    ];
    expect(filterItems(items, "tuio").map((i) => i.id)).toEqual(["1"]);
    expect(filterItems(items, "sesión disparó").map((i) => i.id)).toEqual(["2"]);
    expect(filterItems(items, "").length).toBe(2);
  });
});

import { niceStep } from "../../components/Charts";
describe("niceStep", () => {
  it("redondea el paso del eje", () => {
    expect(niceStep(11 / 3)).toBe(5);
    expect(niceStep(15)).toBe(20);
    expect(niceStep(0.3)).toBeCloseTo(0.5);
    expect(niceStep(0)).toBe(1);
  });
});

import { tickFormat } from "../../components/Charts";
import { fmt as f2 } from "../../lib/format";
describe("tickFormat", () => {
  it("marcas cortas para importes y cantidades grandes", () => {
    const usd = tickFormat(f2.usd);
    expect([0, 2.5, 250, 1500, 15000].map(usd)).toEqual(["$0", "$2.50", "$250", "$1.5K", "$15K"]);
    const n = tickFormat(f2.int);
    expect([500, 15000].map(n)).toEqual(["500", "15K"]);
  });
});
