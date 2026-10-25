import { describe, expect, it } from "vitest";
import { shouldVirtualize, visibleRange } from "../virtual";

const base = { scrollTop: 0, viewport: 300, offsetTop: 0, rowHeight: 30, count: 1000, overscan: 8 };

describe("filas visibles de una tabla larga", () => {
  it("arriba del todo: la ventana y el margen de abajo", () => {
    expect(visibleRange(base)).toEqual({ start: 0, end: 18 });
  });

  it("en medio: la ventana con margen arriba y abajo", () => {
    expect(visibleRange({ ...base, scrollTop: 3000 })).toEqual({ start: 92, end: 118 });
  });

  it("una fila cortada arriba también se pinta", () => {
    expect(visibleRange({ ...base, scrollTop: 3015, overscan: 0 })).toEqual({ start: 100, end: 111 });
  });

  it("al final: el margen se recorta en el último", () => {
    expect(visibleRange({ ...base, scrollTop: 29_700 })).toEqual({ start: 982, end: 1000 });
  });

  it("con la tabla por debajo de la ventana no pinta nada", () => {
    expect(visibleRange({ ...base, offsetTop: 2000 })).toEqual({ start: 0, end: 0 });
  });

  it("con la tabla empezando a media ventana cuenta su desplazamiento", () => {
    expect(visibleRange({ ...base, offsetTop: 150, overscan: 0 })).toEqual({ start: 0, end: 5 });
  });

  it("con la tabla ya pasada por arriba no pinta nada", () => {
    expect(visibleRange({ ...base, count: 10, scrollTop: 5000 })).toEqual({ start: 10, end: 10 });
  });

  it("sin filas no pinta nada", () => {
    expect(visibleRange({ ...base, count: 0 })).toEqual({ start: 0, end: 0 });
  });

  it("sin altura de fila medida no calcula nada", () => {
    expect(visibleRange({ ...base, rowHeight: 0 })).toEqual({ start: 0, end: 0 });
  });

  it("el margen se recorta arriba sin pasar de 0", () => {
    expect(visibleRange({ ...base, scrollTop: 90 })).toEqual({ start: 0, end: 21 });
  });
});

describe("cuándo virtualizar", () => {
  it.each([
    [0, false],
    [199, false],
    [200, false],
    [201, true],
    [1000, true],
  ])("%i filas → %s", (n, expected) => {
    expect(shouldVirtualize(n)).toBe(expected);
  });
});
