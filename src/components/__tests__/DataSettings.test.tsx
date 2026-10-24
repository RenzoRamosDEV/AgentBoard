import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { createElement } from "react";
import { DataSettings } from "../DataSettings";
import { setLang } from "../../lib/i18n";

const html = (props: Partial<Parameters<typeof DataSettings>[0]> = {}) =>
  renderToStaticMarkup(
    createElement(DataSettings, {
      memoryOnly: false,
      rebuilding: false,
      onToggle: vi.fn(),
      onRebuild: vi.fn(),
      ...props,
    }),
  );

describe("pestaña Datos", () => {
  it("el interruptor refleja «Solo memoria»", () => {
    setLang("es");
    expect(html({ memoryOnly: true })).toContain('aria-checked="true"');
    expect(html({ memoryOnly: false })).toContain('aria-checked="false"');
    expect(html()).toContain("Solo memoria");
    expect(html()).toContain("al abrir la app se vuelven a leer todos los logs");
  });

  it("el botón de reconstruir se deshabilita mientras reconstruye", () => {
    setLang("es");
    expect(html({ rebuilding: false })).toMatch(/<button[^>]*>Reconstruir caché<\/button>/);
    expect(html({ rebuilding: true })).toMatch(/<button[^>]*disabled=""[^>]*>Reconstruyendo…<\/button>/);
  });

  it("se muestra en inglés, portugués y francés", () => {
    for (const [lang, word] of [
      ["en", "Memory only"],
      ["pt", "Só memória"],
      ["fr", "Mémoire uniquement"],
    ] as const) {
      setLang(lang);
      const out = html();
      expect(out).toContain(word);
      expect(out).not.toContain("Reconstruir caché");
      expect(out).not.toContain("se vuelven a leer");
    }
    setLang("es");
  });
});
