import { describe, expect, it } from "vitest";
import { compactionAlert, timelinePoints } from "../SessionDetail";
import { setLang } from "../../lib/i18n";

describe("detalle de sesión compacto", () => {
  it("la línea de coste llega en columnas", () => {
    expect(timelinePoints({ ts: [10, 20, 30], cost: [0.5, 1.25, 2] })).toEqual([
      { ts: 10, costUsd: 0.5 },
      { ts: 20, costUsd: 1.25 },
      { ts: 30, costUsd: 2 },
    ]);
    expect(timelinePoints({ ts: [], cost: [] })).toEqual([]);
  });

  it("el aviso tras compactar usa el cálculo del backend", () => {
    setLang("es");
    expect(compactionAlert({ k: 3, ratio: 3 })).toContain("Tras la 3.ª compactación, cada turno cuesta 3.0× más");
    expect(compactionAlert({ k: 1, ratio: 1.19 })).toBeNull();
    expect(compactionAlert({ k: 1, ratio: null })).toBeNull();
    expect(compactionAlert(null)).toBeNull();
  });
});
