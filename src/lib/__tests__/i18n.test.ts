import { describe, expect, it } from "vitest";
import { resolveLang, setLang, systemLang, t } from "../i18n";

describe("i18n", () => {
  it("traduce y sustituye variables", () => {
    setLang("en");
    expect(t("Coste")).toBe("Cost");
    expect(t("{n} llamadas", { n: 5 })).toBe("5 calls");
    setLang("es");
    expect(t("Coste")).toBe("Coste"); // el español es la clave
    expect(t("{n} llamadas", { n: 3 })).toBe("3 llamadas");
  });

  it("una clave sin traducir cae al español", () => {
    setLang("fr");
    expect(t("clave inexistente 123")).toBe("clave inexistente 123");
    setLang("es");
  });

  it("resolveLang devuelve el idioma del sistema para 'system'", () => {
    expect(["es", "en", "pt", "fr"]).toContain(resolveLang("system"));
    expect(resolveLang("pt")).toBe("pt");
    expect(["es", "en", "pt", "fr"]).toContain(systemLang());
  });
});
