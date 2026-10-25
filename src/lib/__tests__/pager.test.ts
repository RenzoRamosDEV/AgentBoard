import { describe, expect, it } from "vitest";
import { createPager, type PagerState } from "../pager";
import type { Page } from "../api";

/** Servidor falso: 7 filas numeradas, ordenadas según `sort`; las respuestas se resuelven a mano. */
function fakeServer() {
  const pending: { resolve: () => void }[] = [];
  const fetch = (offset: number, limit: number, sort: string) =>
    new Promise<Page<string>>((resolve) => {
      const all = Array.from({ length: 7 }, (_, i) => `${sort}-${i}`);
      pending.push({ resolve: () => resolve({ rows: all.slice(offset, offset + limit), total: 7, offset }) });
    });
  return { fetch, flush: async (i = 0) => { pending.splice(i, 1)[0].resolve(); await new Promise((r) => setTimeout(r, 0)); }, pending };
}

describe("páginas de una lista del servidor", () => {
  it("carga la primera página, añade las siguientes y reinicia al cambiar el orden", async () => {
    const server = fakeServer();
    const states: PagerState<string>[] = [];
    const pager = createPager<string, string>(server.fetch, 3, (s) => states.push(s));
    pager.load("coste");
    await server.flush();
    expect(pager.state()).toEqual({ rows: ["coste-0", "coste-1", "coste-2"], total: 7 });
    pager.more();
    await server.flush();
    expect(pager.state().rows).toHaveLength(6);
    // Otro orden (o un `data:changed`): vuelve a la primera página.
    pager.load("inicio");
    expect(pager.state().rows).toHaveLength(6);
    await server.flush();
    expect(pager.state()).toEqual({ rows: ["inicio-0", "inicio-1", "inicio-2"], total: 7 });
    expect(states.length).toBeGreaterThanOrEqual(3);
  });

  it("descarta una página que llega tarde, de un orden anterior", async () => {
    const server = fakeServer();
    const pager = createPager<string, string>(server.fetch, 3, () => {});
    pager.load("a");
    await server.flush();
    pager.more();
    pager.load("b");
    await server.flush(0); // la página 2 de «a» llega después de pedir «b»
    expect(pager.state().rows).toEqual(["a-0", "a-1", "a-2"]);
    await server.flush(0);
    expect(pager.state().rows).toEqual(["b-0", "b-1", "b-2"]);
  });

  it("no pide más cuando ya están todas", async () => {
    const server = fakeServer();
    const pager = createPager<string, string>(server.fetch, 5, () => {});
    pager.load("x");
    await server.flush();
    pager.more();
    await server.flush();
    expect(pager.state().rows).toHaveLength(7);
    pager.more();
    expect(server.pending).toHaveLength(0);
  });
});
