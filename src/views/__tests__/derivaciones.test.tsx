import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { createElement } from "react";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { setLang } from "../../lib/i18n";
import { Overview } from "../Overview";
import { Section } from "../Section";
import type { DetailSectionId } from "../../lib/sections";
import { fixture, NOW } from "./fixture";

const goldenPath = new URL("./__golden__/derivaciones.json", import.meta.url);
const SECTIONS: DetailSectionId[] = ["daily", "agent", "project", "model", "activity", "tools", "shell", "skills", "mcp", "agents"];

function renderAll() {
  const out: Record<string, string> = {};
  for (const [name, days, kind] of [["30d", 30, "30d"], ["todo", 450, "all"]] as const) {
    const data = fixture(days);
    const period = { kind };
    out[`${name}/overview`] = renderToStaticMarkup(createElement(Overview, { data, period, budget: 100, singleProject: null, open: () => {}, openSession: () => {} }));
    for (const id of SECTIONS) out[`${name}/${id}`] = renderToStaticMarkup(createElement(Section, { id, data, period, singleProject: null, back: () => {} }));
  }
  return out;
}

describe("derivaciones de los paneles (golden grabado antes de memoizarlas)", () => {
  beforeAll(() => {
    vi.useFakeTimers({ now: NOW, toFake: ["Date"] });
    setLang("es");
  });
  afterAll(() => vi.useRealTimers());

  it("Resumen y apartados pintan lo mismo con los datos de ejemplo", () => {
    const out = renderAll();
    if (process.env.UPDATE_GOLDEN === "1" || !existsSync(goldenPath)) writeFileSync(goldenPath, JSON.stringify(out, null, 1));
    const golden = JSON.parse(readFileSync(goldenPath, "utf8")) as Record<string, string>;
    expect(Object.keys(out)).toEqual(Object.keys(golden));
    for (const k of Object.keys(golden)) expect(out[k], k).toBe(golden[k]);
  });
});
