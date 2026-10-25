import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

const app = readFileSync(new URL("../../App.tsx", import.meta.url), "utf8");

describe("carga por apartado", () => {
  it.each([
    ["Section", "./views/Section"],
    ["Projects", "./views/Projects"],
    ["ProjectDetail", "./views/ProjectDetail"],
    ["SessionDetail", "./views/SessionDetail"],
    ["Pricing", "./views/Pricing"],
    ["SettingsDialog", "./components/SettingsDialog"],
    ["CommandPalette", "./components/CommandPalette"],
  ])("%s se importa solo con lazy(() => import(...))", (_, path) => {
    const escaped = path.replace(/[./]/g, (c) => `\\${c}`);
    expect(app).not.toMatch(new RegExp(`^import [^;]*from "${escaped}";`, "m"));
    expect(app).toMatch(new RegExp(`lazy\\(\\(\\) =>\\s*import\\("${escaped}"\\)`));
  });

  it("el Resumen sigue en el chunk inicial", () => {
    expect(app).toMatch(/^import [^;]*from "\.\/views\/Overview";/m);
  });
});
