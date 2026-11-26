import { describe, expect, it } from "vitest";
import { readFileSync, writeFileSync } from "node:fs";

const css = readFileSync(new URL("../../styles.css", import.meta.url), "utf8");

const squash = (s: string) => s.replace(/\s+/g, " ").trim();

/**
 * Declaraciones de primer nivel (fuera de @media) cuyo selector, o alguno de su lista
 * separada por comas, es exactamente `selector`. Si aparece varias veces, gana la última.
 */
function declarations(source: string, selector: string): Record<string, string> {
  const text = source.replace(/\/\*[\s\S]*?\*\//g, "");
  const out: Record<string, string> = {};
  let depth = 0;
  let start = 0;
  let prelude = "";
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (ch === "{") {
      if (depth === 0) prelude = text.slice(start, i);
      depth++;
      start = i + 1;
    } else if (ch === "}") {
      depth--;
      if (depth === 0) {
        const body = text.slice(start, i);
        const selectors = prelude.split(",").map(squash);
        if (!prelude.trim().startsWith("@") && selectors.includes(squash(selector))) {
          for (const decl of body.split(";")) {
            const colon = decl.indexOf(":");
            if (colon < 0) continue;
            out[squash(decl.slice(0, colon))] = squash(decl.slice(colon + 1));
          }
        }
      }
      start = i + 1;
    }
  }
  return out;
}

const DARK = ":root";
const LIGHT = ':root[data-theme="light"]';

describe("helper declarations", () => {
  const sample = `
    /* comentario { con llaves } */
    .a { color: red; }
    .b, .a  .c { margin:  0  1px ; }
    @media (max-width: 10px) { .a { color: green; } }
    .a { color: blue; padding: 2px }
  `;

  it("devuelve un mapa vacío si el selector no existe", () => {
    expect(declarations(sample, ".zzz")).toEqual({});
  });

  it("si el selector se repite gana la última declaración y se ignoran las de @media", () => {
    expect(declarations(sample, ".a")).toEqual({ color: "blue", padding: "2px" });
  });

  it("encuentra el selector dentro de una lista y normaliza espacios", () => {
    expect(declarations(sample, ".a .c")).toEqual({ margin: "0 1px" });
  });
});

describe("colores de series intactos", () => {
  const SERIES_TOKEN = /^--(palette|series|act|agent)-/;
  const seriesTokens = (selector: string) =>
    Object.fromEntries(Object.entries(declarations(css, selector)).filter(([prop]) => SERIES_TOKEN.test(prop)).sort(([a], [b]) => a.localeCompare(b)));

  it("los --palette-*, --series-*, --act-* y --agent-* coinciden con el golden en ambos modos", () => {
    const goldenUrl = new URL("./__golden__/series-tokens.json", import.meta.url);
    const out = JSON.stringify({ dark: seriesTokens(DARK), light: seriesTokens(LIGHT) }, null, 2) + "\n";
    if (process.env.UPDATE_GOLDEN === "1") writeFileSync(goldenUrl, out);
    expect(out).toBe(readFileSync(goldenUrl, "utf8"));
  });

  it.each([DARK, LIGHT])("%s declara 30 tokens de series (8 palette, 6 series, 11 act, 5 agent)", (selector) => {
    const counts = Object.keys(seriesTokens(selector)).reduce<Record<string, number>>((acc, prop) => {
      const family = prop.split("-")[2];
      acc[family] = (acc[family] ?? 0) + 1;
      return acc;
    }, {});
    expect(counts).toEqual({ palette: 8, series: 6, act: 11, agent: 5 });
  });
});

function relativeLuminance(hex: string): number {
  const channels = hex.replace("#", "").match(/../g)!.map((h) => parseInt(h, 16) / 255);
  const [r, g, b] = channels.map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

describe("paleta Grafito", () => {
  const palette = {
    [DARK]: {
      "--bg": "#0f0f10",
      "--sidebar": "#0f0f10",
      "--surface-1": "#161618",
      "--surface-2": "#1c1c1f",
      "--surface-3": "#232326",
      "--border": "#262629",
      "--row-border": "#1e1e21",
      "--text-primary": "#ededee",
      "--text-secondary": "#b4b4b8",
      "--track": "#232326",
    },
    [LIGHT]: {
      "--bg": "#f4f4f5",
      "--sidebar": "#f4f4f5",
      "--surface-1": "#ffffff",
      "--surface-2": "#f0f0f2",
      "--surface-3": "#ececef",
      "--border": "#e4e4e7",
      "--row-border": "#f0f0f2",
      "--text-primary": "#111113",
      "--text-secondary": "#4a4a50",
      "--track": "#ececef",
    },
  };

  it.each(Object.entries(palette))("%s usa los tokens neutros de Grafito", (selector, expected) => {
    const decls = declarations(css, selector);
    const actual = Object.fromEntries(Object.keys(expected).map((prop) => [prop, decls[prop]]));
    expect(actual).toEqual(expected);
  });

  it.each([DARK, LIGHT])("%s: --text-muted contrasta al menos 4.5:1 con --bg", (selector) => {
    const decls = declarations(css, selector);
    expect(contrast(decls["--text-muted"], decls["--bg"])).toBeGreaterThanOrEqual(4.5);
  });

  it.each([
    [DARK, "#848489"],
    [LIGHT, "#6b6b73"],
  ])("%s: --text-muted es el tono AA más cercano al de la maqueta (%s)", (selector, expected) => {
    expect(declarations(css, selector)["--text-muted"]).toBe(expected);
  });

  it("el atenuado original de la maqueta no llega a AA sobre el fondo oscuro", () => {
    expect(contrast("#77777d", "#0f0f10")).toBeLessThan(4.5);
    expect(contrast("#77777d", "#0f0f10")).toBeCloseTo(4.31, 2);
    expect(contrast("#ffffff", "#000000")).toBeCloseTo(21, 5);
  });
});

describe("tarjetas rectas con borde degradado", () => {
  it.each([
    [DARK, "#f2a33a"],
    [LIGHT, "#20b0f0"],
  ])("%s: --card-edge vale %s", (selector, expected) => {
    expect(declarations(css, selector)["--card-edge"]).toBe(expected);
  });

  it.each([".panel", ".kpi"])("%s es recta, transparente, sin borde ni sombra y ancla el borde", (selector) => {
    expect(declarations(css, selector)).toMatchObject({
      "border-radius": "0",
      background: "transparent",
      "box-shadow": "none",
      border: "0",
      position: "relative",
    });
  });

  it.each([".panel::before", ".kpi::before"])("%s pinta un borde de 1 px degradado que no captura el ratón", (selector) => {
    expect(declarations(css, selector)).toMatchObject({
      content: '""',
      position: "absolute",
      inset: "0",
      padding: "1px",
      "pointer-events": "none",
      background: "linear-gradient(135deg, var(--card-edge), transparent 40%, transparent 65%, var(--border))",
      "-webkit-mask": "linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0)",
      mask: "linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0)",
      "-webkit-mask-composite": "xor",
      "mask-composite": "exclude",
    });
  });

  it.each([
    [".panel", "19px 21px"],
    [".kpi", "15px 17px"],
    [".kpis-compact .kpi", "13px 15px"],
  ])("%s compensa con 1 px más de padding el borde retirado (%s)", (selector, expected) => {
    expect(declarations(css, selector).padding).toBe(expected);
  });
});

describe("tarjetas en ventanas estrechas", () => {
  it("el padding compacto de .panel (max-width: 860px) también compensa 1 px", () => {
    const media = css.match(/@media \(max-width: 860px\) \{([\s\S]*?)\n\}/)?.[1] ?? "";
    expect(declarations(media, ".panel").padding).toBe("15px");
  });
});
