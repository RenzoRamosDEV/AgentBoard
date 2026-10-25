import { describe, expect, it } from "vitest";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gzipSync } from "node:zlib";
import { median, parseResults, summarize } from "../perfStats";
import { chunkSizes, initialChunks } from "../../../scripts/perf-ui.mjs";

describe("mediana de repeticiones", () => {
  it.each([
    [[], null],
    [[7], 7],
    [[3, 1], 2],
    [[5, 1, 3], 3],
    [[4, 1, 3, 2], 2.5],
  ])("median(%j) = %j", (xs, expected) => {
    expect(median(xs)).toBe(expected);
  });

  it("no altera el orden de la entrada", () => {
    const xs = [3, 1, 2];
    median(xs);
    expect(xs).toEqual([3, 1, 2]);
  });
});

describe("resumen de repeticiones", () => {
  it("con un valor, mediana y máximo son ese valor", () => {
    expect(summarize([12.345])).toEqual({ n: 1, median: 12.3, max: 12.3 });
  });

  it("con cinco valores da mediana y máximo redondeados a décimas", () => {
    expect(summarize([40.04, 10, 30.06, 20, 50.55])).toEqual({ n: 5, median: 30.1, max: 50.6 });
  });

  it("sin valores no hay cifras", () => {
    expect(summarize([])).toEqual({ n: 0, median: null, max: null });
  });
});

describe("resultados del arnés en el DOM volcado", () => {
  it("extrae el JSON de <pre id=\"perf-results\"> con entidades HTML", () => {
    const html = `<html><body><div id="root"></div><pre id="perf-results">{"S1":{"label":"a &amp; b &lt;c&gt; \\&quot;d\\&quot;"}}</pre></body></html>`;
    expect(parseResults(html)).toEqual({ S1: { label: 'a & b <c> "d"' } });
  });

  it("falla si el arnés no escribió resultados", () => {
    expect(() => parseResults("<html><body><div id=\"root\"></div></body></html>")).toThrow(/perf-results/);
  });
});

describe("tamaño de los chunks", () => {
  it("da bytes y gzip de cada archivo .js y .css, del mayor al menor", () => {
    const dir = mkdtempSync(join(tmpdir(), "perf-chunks-"));
    try {
      const big = "const a = 1;\n".repeat(200);
      writeFileSync(join(dir, "index-abc.js"), big);
      writeFileSync(join(dir, "index-abc.css"), "body{}");
      writeFileSync(join(dir, "logo.png"), "png");
      mkdirSync(join(dir, "sub"));
      expect(chunkSizes(dir)).toEqual([
        { file: "index-abc.js", bytes: big.length, gzip: gzipSync(big).length },
        { file: "index-abc.css", bytes: 6, gzip: gzipSync("body{}").length },
      ]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("chunks del arranque", () => {
  it("son el script, los modulepreload y la hoja de estilos de index.html", () => {
    const html = `<script type="module" crossorigin src="/assets/index-a.js"></script>
    <link rel="modulepreload" crossorigin href="/assets/react-b.js">
    <link rel="stylesheet" crossorigin href="/assets/index-c.css">
    <link rel="icon" href="/favicon.png">`;
    expect(initialChunks(html)).toEqual(new Set(["index-a.js", "react-b.js", "index-c.css"]));
  });

  it("sin referencias no hay ninguno", () => {
    expect(initialChunks("<html></html>")).toEqual(new Set());
  });
});
