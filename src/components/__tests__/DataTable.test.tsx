import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { createElement } from "react";
import { DataTable, nextSort, type Column } from "../DataTable";
import { setLang } from "../../lib/i18n";

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
