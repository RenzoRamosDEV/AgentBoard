import { describe, expect, it } from "vitest";
import { rankItems, rankKeys, stacksByBucket } from "../derive";
import { readFileSync } from "node:fs";

interface R {
  key: string;
  v: number;
}
const rows: R[] = [
  { key: "a", v: 1 },
  { key: "b", v: 5 },
  { key: "c", v: 3 },
  { key: "d", v: 2 },
];
const get = (r: R) => r.v;
const keys = (xs: { r: R }[]) => xs.map((x) => x.r.key);

describe("ranking con «Otros»", () => {
  it("con límite 1 todo va a «Otros» (suma)", () => {
    const out = rankItems(rows, get, false, 1);
    expect(keys(out.head)).toEqual([]);
    expect(keys(out.tail)).toEqual(["b", "c", "d", "a"]);
    expect(out.others).toBe(11);
  });

  it("con límite 2 queda la primera y el resto en «Otros»", () => {
    const out = rankItems(rows, get, false, 2);
    expect(keys(out.head)).toEqual(["b"]);
    expect(out.others).toBe(6);
  });

  it("con límite igual al número de filas no hay «Otros»", () => {
    const out = rankItems(rows, get, false, 4);
    expect(keys(out.head)).toEqual(["b", "c", "d", "a"]);
    expect(out.tail).toEqual([]);
    expect(out.others).toBeNull();
  });

  it("con una métrica de tasa «Otros» es la media", () => {
    expect(rankItems(rows, get, true, 2).others).toBe(2);
  });

  it("conserva el índice original de cada fila (para su color)", () => {
    expect(rankItems(rows, get, false, 4).head.map((x) => x.i)).toEqual([1, 2, 3, 0]);
  });

  it("con empate mantiene el orden de entrada", () => {
    const tie = [{ key: "x", v: 1 }, { key: "y", v: 1 }];
    expect(keys(rankItems(tie, get, false, 5).head)).toEqual(["x", "y"]);
  });
});

interface S {
  ts: number;
  key: string;
  label: string;
  v: number;
}
const series: S[] = [
  { ts: 10, key: "a", label: "A", v: 1 },
  { ts: 11, key: "b", label: "B", v: 4 },
  { ts: 20, key: "a", label: "A2", v: 2 },
  { ts: 21, key: "a", label: "A3", v: 3 },
  { ts: 22, key: "c", label: "C", v: 0.5 },
];
const bucket = (ts: number) => Math.floor(ts / 10) * 10;

describe("ranking de claves de una serie", () => {
  it("suma por clave, ordena de mayor a menor y usa la primera etiqueta", () => {
    expect(rankKeys(series, (s) => s.v, (s) => s.label)).toEqual([
      { key: "a", label: "A", value: 6 },
      { key: "b", label: "B", value: 4 },
      { key: "c", label: "C", value: 0.5 },
    ]);
  });

  it("sin serie no hay claves", () => {
    expect(rankKeys([] as S[], (s) => s.v, (s) => s.label)).toEqual([]);
  });
});

describe("pilas por cubo", () => {
  it("suma por cubo y clave en orden de aparición; etiqueta y color de la primera", () => {
    const out = stacksByBucket(series, (s) => bucket(s.ts), (s) => s.key, (s) => s.label, (s) => s.v, (s) => `col-${s.key}`);
    expect([...out.entries()]).toEqual([
      [10, [{ key: "a", label: "A", value: 1, color: "col-a" }, { key: "b", label: "B", value: 4, color: "col-b" }]],
      [20, [{ key: "a", label: "A2", value: 5, color: "col-a" }, { key: "c", label: "C", value: 0.5, color: "col-c" }]],
    ]);
  });

  it("agrupa claves en una sola si la función de clave las une («Otros»)", () => {
    const out = stacksByBucket(series, () => 0, (s) => (s.key === "a" ? "a" : "__otros"), (s) => s.label, (s) => s.v, () => "c");
    expect(out.get(0)).toEqual([{ key: "a", label: "A", value: 6, color: "c" }, { key: "__otros", label: "B", value: 4.5, color: "c" }]);
  });
});

describe("los gráficos no animan su geometría", () => {
  const charts = readFileSync(new URL("../../components/Charts.tsx", import.meta.url), "utf8");
  const css = readFileSync(new URL("../../styles.css", import.meta.url), "utf8");
  const chartClasses = new Set(
    [...charts.matchAll(/className=\{?[`"]([^`"]*)/g)].flatMap((m) => m[1].split(/\s+/)).filter((c) => /^[a-z][\w-]*$/.test(c) && c !== "muted" && c !== "num"),
  );

  it("Charts.tsx no usa <animate> ni transiciones en línea", () => {
    expect(charts).not.toMatch(/<animate/);
    expect(charts).not.toMatch(/transition/);
    expect(chartClasses.size).toBeGreaterThan(10);
  });

  it("ninguna regla de styles.css de los gráficos anima tamaño ni posición", () => {
    const geometry = /\b(all|width|height|transform|left|top|right|bottom|x|y|d|flex(-basis)?|margin|padding|inset)\b/;
    const offenders: string[] = [];
    for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const selector = m[1];
      const classes = [...selector.matchAll(/\.([\w-]+)/g)].map((x) => x[1]);
      if (!classes.some((c) => chartClasses.has(c))) continue;
      for (const decl of m[2].split(";")) {
        const [prop, value = ""] = decl.split(":").map((x) => x.trim());
        if (prop === "animation" || prop === "animation-name") offenders.push(`${selector.trim()} { ${decl.trim()} }`);
        if ((prop === "transition" || prop === "transition-property") && value.split(",").some((part) => geometry.test(part.trim().split(/\s+/)[0])))
          offenders.push(`${selector.trim()} { ${decl.trim()} }`);
      }
    }
    expect(offenders).toEqual([]);
  });
});
