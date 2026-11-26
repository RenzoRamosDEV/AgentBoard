import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

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
