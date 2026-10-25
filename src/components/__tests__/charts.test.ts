import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

describe("los gráficos no animan su geometría", () => {
  const charts = readFileSync(new URL("../Charts.tsx", import.meta.url), "utf8");
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
