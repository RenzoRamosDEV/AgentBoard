import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { createElement } from "react";
import { SettingsDialog } from "../SettingsDialog";
import type { Settings } from "../../lib/api";
import { setLang } from "../../lib/i18n";

const settings = { theme: "system", language: "es" } as Settings;

const temaHtml = () => {
  setLang("es");
  return renderToStaticMarkup(
    createElement(SettingsDialog, { settings, onSave: vi.fn(), onClose: vi.fn(), onExport: vi.fn() }),
  );
};

const previews = (html: string) => html.match(/<svg[^>]*viewBox="0 0 160 100"[^>]*>[\s\S]*?<\/svg>/g) ?? [];

describe("miniatura de tema en Ajustes", () => {
  it("dibuja una miniatura por tema claro y oscuro, dos en «Sistema»", () => {
    expect(previews(temaHtml())).toHaveLength(4);
  });

  it("el lienzo de cada miniatura no tiene radio de borde", () => {
    for (const svg of previews(temaHtml())) {
      const open = svg.match(/<svg[^>]*>/)![0];
      const radius = open.match(/border-radius:\s*([^;"]+)/);
      if (radius) expect(radius[1].trim()).toMatch(/^0(px)?$/);
    }
  });

  it("ninguna forma dibujada tiene esquinas redondeadas", () => {
    const html = previews(temaHtml()).join("");
    const radii = [...html.matchAll(/\s(rx|ry)="([^"]*)"/g)].map((m) => m[2]);
    expect(radii.filter((v) => Number(v) !== 0)).toEqual([]);
  });

  it("el logotipo sigue siendo un círculo y los colores no cambian", () => {
    const svgs = previews(temaHtml());
    for (const svg of svgs) expect(svg).toMatch(/<circle[^>]*r="9"/);
    const html = svgs.join("");
    for (const color of ["#f2a33a", "#5b8def", "#20b0f0", "#2a78d6", "#1d2026", "#ffffff"]) {
      expect(html).toContain(`fill="${color}"`);
    }
  });
});
