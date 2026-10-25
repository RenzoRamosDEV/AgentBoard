import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { createElement } from "react";
import { DataTable, nextSort, type Column } from "../DataTable";
import { setLang } from "../../lib/i18n";
import { readFileSync, writeFileSync } from "node:fs";

interface Row {
  id: string;
  v: number;
}

const columns: Column<Row>[] = [
  { header: "Id", cell: (r) => `fila-${r.id}` },
  { header: "V", cell: (r) => String(r.v), sort: (r) => r.v },
];

const html = (props: Partial<Parameters<typeof DataTable<Row>>[0]>) =>
  renderToStaticMarkup(
    createElement(DataTable<Row>, {
      rows: [
        { id: "b", v: 1 },
        { id: "a", v: 2 },
      ],
      rowKey: (r: Row) => r.id,
      columns,
      ...props,
    }),
  );

describe("tabla con orden y páginas en el servidor", () => {
  it("en modo controlado no ordena en el cliente", () => {
    const out = html({ sort: { header: "V", dir: "desc" }, onSortChange: vi.fn() });
    expect(out.indexOf("fila-b")).toBeLessThan(out.indexOf("fila-a"));
    expect(out).toContain('aria-sort="descending"');
    // Sin control, ordena en el cliente como siempre.
    const local = html({ defaultSort: { header: "V", dir: "desc" } });
    expect(local.indexOf("fila-a")).toBeLessThan(local.indexOf("fila-b"));
  });

  it("el siguiente orden alterna la dirección de la misma columna", () => {
    expect(nextSort(undefined, "V")).toEqual({ header: "V", dir: "desc" });
    expect(nextSort({ header: "V", dir: "desc" }, "V")).toEqual({ header: "V", dir: "asc" });
    expect(nextSort({ header: "V", dir: "asc" }, "V")).toEqual({ header: "V", dir: "desc" });
    expect(nextSort({ header: "V", dir: "asc" }, "Id")).toEqual({ header: "Id", dir: "desc" });
  });

  it("muestra «Cargar más» mientras quedan filas en el servidor", () => {
    setLang("es");
    const onMore = vi.fn();
    expect(html({ more: { total: 10, onMore } })).toContain("Cargar más (2 de 10)");
    expect(html({ more: { total: 2, onMore } })).not.toContain("Cargar más");
    setLang("en");
    expect(html({ more: { total: 10, onMore } })).toContain("Load more (2 of 10)");
    setLang("es");
  });
});

const long = (n: number): Row[] => Array.from({ length: n }, (_, i) => ({ id: `r${i}`, v: (i * 37) % 101 }));
const goldenPath = new URL("./__golden__/datatable-200.html", import.meta.url);
const rowsIn = (out: string) => (out.match(/class="table-row /g) ?? []).length;

describe("tablas largas", () => {
  it("con 200 filas el marcado es el de siempre (golden), ordenado en el cliente", () => {
    const out = html({ rows: long(200), collapse: false, defaultSort: { header: "V", dir: "desc" }, onRowClick: vi.fn(), rowClassName: (r) => (r.v > 50 ? "alto" : "") });
    if (process.env.UPDATE_GOLDEN === "1") writeFileSync(goldenPath, out);
    expect(out).toBe(readFileSync(goldenPath, "utf8"));
    expect(rowsIn(out)).toBe(200);
    expect(out).not.toContain("aria-rowcount");
  });

  it("con 1000 filas desplegadas pinta solo la ventana inicial y dos espaciadores con la altura del resto", () => {
    const out = html({ rows: long(1000), collapse: false });
    const painted = rowsIn(out);
    expect(painted).toBeGreaterThan(0);
    expect(painted).toBeLessThanOrEqual(60);
    expect(out).toContain('aria-rowcount="1000"');
    expect(out).toContain('aria-rowindex="1"');
    expect(out).toContain(`aria-rowindex="${painted}"`);
    const spacers = [...out.matchAll(/<div aria-hidden="true" class="table-spacer" style="height:(\d+(?:\.\d+)?)(?:px)?"><\/div>/g)].map((m) => Number(m[1]));
    expect(spacers).toHaveLength(2);
    expect(spacers[0]).toBe(0);
    expect(spacers[1] / (1000 - painted)).toBeGreaterThan(20);
    expect(Number.isInteger(spacers[1] / (1000 - painted))).toBe(true);
  });

  it("con 201 filas ya se virtualiza", () => {
    expect(html({ rows: long(201), collapse: false })).toContain('aria-rowcount="201"');
  });

  it("no vuelve a ordenar las mismas filas si solo cambia otro prop", () => {
    let calls = 0;
    const counted: Column<Row>[] = [columns[0], { ...columns[1], sort: (r) => (calls++, r.v) }];
    const rows = long(50);
    const props = { rows, columns: counted, collapse: false as const, defaultSort: { header: "V", dir: "asc" as const } };
    const first = html({ ...props, empty: "uno" });
    const afterFirst = calls;
    const second = html({ ...props, empty: "dos" });
    expect(afterFirst).toBeGreaterThan(0);
    expect(calls).toBe(afterFirst);
    expect(second).toBe(first);
  });
});
