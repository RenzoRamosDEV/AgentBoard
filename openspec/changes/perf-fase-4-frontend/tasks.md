# Tasks

Comandos de Rust dentro de distrobox: `distrobox enter agentburn-dev -- bash -lc 'export PATH=$HOME/.cargo/bin:$PATH; cd src-tauri && <cmd>'`. Sin compilaciones `release`.
**TDD**: en cada tarea que cambia comportamiento, primero el test en rojo (verificar que falla), luego el código mínimo en verde y después refactor. `npm test` en verde al cerrar cada tarea.
**Medición (D1)**: `node scripts/perf-ui.mjs` (build `perf`, latencia 104 ms; 3 ejecuciones × 5 repeticiones, mediana). **Regla de conservar/revertir (D1)**: un paso se conserva si mejora su escenario ≥ 10 % sin empeorar otro > 10 %, o si es estructural y no empeora > 10 %; si no, se revierte (`git revert` del paso) y se anota en «Desviaciones durante la implementación» de design.md.
**Aspecto**: tras cada grupo, `npm run dev` (mock) en `?s=<apartado>` y comprobar que cada apartado tocado se ve igual que antes.

## 1. Arnés de medición y partida

- [x] 1.1 Tests en rojo: `src/dev/__tests__/perfHarness.test.ts` para las funciones puras del arnés (`median`, `summarize` de repeticiones con 1 y 5 valores, y `parseResults` que extrae el JSON de `<pre id="perf-results">` y falla sin él); `scripts/perf-ui.mjs` exporta `chunkSizes(dir)` probada sobre una carpeta temporal (bytes y gzip); verificar que fallan
- [x] 1.2 Implementar D1: `.env.perf` (`VITE_MOCK=1`), instalación del mock en `main.tsx` con `VITE_MOCK`, `?latency=` en `mock.ts`, `src/dev/perfHarness.ts` (Profiler, escenarios S1–S4 con clics y `data-perf`), `scripts/perf-ui.mjs`; verificar 1.1 en verde, que `npm run build` (sin modo `perf`) no incluye el mock ni el arnés (buscar `installMocks` en `dist/assets`) y que `node scripts/perf-ui.mjs` imprime S1–S4 y los chunks
- [x] 1.3 Medición de partida (antes de cualquier cambio de rendimiento) con latencia 104 y 630 ms; anotar cifras, chunks y commit de partida en design.md («Desviaciones…» → «Partida»)

## 2. Caché de consultas y generación (D2, D3)

- [x] 2.1 Tests en rojo en `src/lib/__tests__/query.test.ts` (reloj y `fetch` falsos): misma clave simultánea → una petición; entrada fresca no se vuelve a pedir; `setGeneration` vuelve a pedir solo las claves con suscriptores y marca obsoletas las demás, que se piden al suscribirse mostrando antes el dato anterior; respuesta de una generación superada no pisa la nueva; `invalidateAll`; `staleMs` en 599 999 / 600 000 / 600 001 ms; LRU con `maxEntries` 2 descarta la menos usada sin suscriptores; error guardado sin perder el dato anterior; y `filterKey` igual para filtros iguales con listas en otro orden de propiedades y distinto si cambia `from`, `to`, `agents` o `projects`; verificar que fallan
- [x] 2.2 Implementar `src/lib/query.ts` (`createQueryStore`, `useQuery`, `useGeneration`, `filterKey`) y en `live.ts` la conexión de `data:changed`/`ingest://done` con `setGeneration` (test de `live.test.ts` adaptado: el aviso cambia la generación y el resalte en vivo sigue igual); verificar 2.1 y `live.test.ts` en verde
- [x] 2.3 Migrar a `useQuery` `useDashboardData`, las listas de agentes/proyectos y `summary({})` de `App`, `Projects`, `ProjectDetail` (detalle; el `pager` se reinicia con `useGeneration`), `SessionDetail`, `Pricing`, `TodayLive` y `CommandPalette`; quitar `refresh` de sus props; `saveSettings` llama a `invalidateAll`; test en `useData.test.ts` de que `loadDashboard` sigue haciendo un único `invoke`; verificar `npm test`, `npm run build`, el aspecto, y medir S1–S4 (S4: solo peticiones de la vista montada) aplicando la regla

## 3. Filtro global ligero y transiciones (D4)

- [x] 3.1 Tests en rojo en `src/lib/__tests__/filterStore.test.ts`: `toggleAgent`/`toggleProject`/`onlyProject`/`setPeriod`/`setCompare` producen el estado esperado; un oyente de un selector no se llama si cambia otra parte del estado; el `Filter` derivado es el mismo objeto mientras no cambian sus entradas (y su `filterKey` estable); `compare` se lee y guarda en un `localStorage` falso y sin almacenamiento vale `true`; `prevFilter` es `null` con «Todo»; verificar que fallan
- [x] 3.2 Implementar `src/lib/filterStore.ts`; `App`, `Topbar` (con `memo`) y las vistas leen de él; cambios de filtro y de apartado en `startTransition`; `useDeferredValue(filter)` en `Overview`/`Section`/`Projects`; `Sidebar` con `memo`; verificar 3.1, `npm test`, `npm run build`, el aspecto, y medir aplicando la regla

## 4. Carga por apartado (D5)

- [x] 4.1 Test en rojo en `src/lib/__tests__/lazy.test.ts`: `App.tsx` importa `Section`, `Projects`, `ProjectDetail`, `SessionDetail`, `Pricing`, `SettingsDialog` y `CommandPalette` solo con `lazy(() => import(...))` y no de forma estática (lectura del fuente, como `live.test.ts`); verificar que falla
- [x] 4.2 Implementar D5 con `Suspense` y el mismo indicador «Cargando…»; verificar 4.1, `npm run build` con un chunk por apartado y el chunk inicial menor que en la partida, el aspecto, y medir S1 en frío y cacheado aplicando la regla

## 5. Tablas (D6)

- [x] 5.1 Tests en rojo en `src/lib/__tests__/virtual.test.ts` de `visibleRange`: arriba del todo, en medio, al final, tabla por debajo de la ventana, `count` 0, `rowHeight` 0, `overscan` recortado en ambos extremos (particiones y bordes 199/200/201 filas para decidir si se virtualiza con `shouldVirtualize`); verificar que fallan
- [x] 5.2 Tests en rojo en `DataTable.test.tsx` (`renderToStaticMarkup`): con 200 filas el marcado es el de hoy (golden de la partida en `src/components/__tests__/__golden__/datatable-200.html`); con 1000 filas desplegadas pinta como mucho la ventana inicial más el margen, dos espaciadores con la altura del resto y `aria-rowcount=1000`; el orden local da el mismo orden que hoy y no vuelve a ordenar si solo cambia un prop ajeno (contador en `sort` de la columna); verificar que fallan
- [x] 5.3 Implementar D6 en `DataTable.tsx` y `src/lib/virtual.ts`; `columns` con `useMemo` en `Projects`, `ProjectDetail`, `SessionDetail` y las vistas ampliadas de desgloses; verificar 5.1–5.2, `npm test`, `npm run build`, el aspecto (incluido desplazarse por una tabla larga de turnos en el mock) y medir aplicando la regla

## 6. Agrupaciones y gráficos (D7)

- [x] 6.1 Tests en rojo: extraer y probar como funciones puras las derivaciones de `RankingCard` (cabeza y «Otros» con métrica suma y de tasa, `limit` 1/2/n), `EvolutionCard` (ranking de claves, `maxKeys`, pilas por cubo) y las pilas de `DailyPanel`/`ActivityFull`/`Overview`, con golden de sus salidas sobre los datos del mock en `src/views/__tests__/__golden__/derivaciones.json` grabado con el código de hoy; test del invariante de animaciones (D7); verificar que fallan
- [x] 6.2 Implementar D7 (`useMemo` por dato y métrica, `memo` en `Charts.tsx`, sin `sort` fuera de memos en `Section.tsx`, `sections.tsx`, `panels.tsx` y `Overview.tsx`); verificar 6.1 con el golden sin regrabar, `npm test`, `npm run build`, el aspecto, y medir aplicando la regla

## 7. Cubo temporal en Rust (D8)

- [ ] 7.1 Tests en rojo en `snapshot.rs` y `tests/snapshot_resumen.rs`: `bucket_for` con periodos de 399/400/401 días, sin `from` (desde el primer dato) y sin datos; `rebucket_month` suma por mes local y clave (cruce de mes con cambio de horario en `Europe/Madrid`, mes sin datos ausente); con cubo diario el snapshot es igual al de hoy en todos los filtros del golden; con cubo mensual cada serie suma por mes lo mismo que la consulta individual (enteros exactos, importes ±1e-6); verificar que fallan
- [ ] 7.2 Implementar D8 en `snapshot.rs` y el tipo en `useData.ts`; verificar 7.1, `equivalencia_consultas` sin regrabar y `cargo test --workspace` en verde
- [ ] 7.3 Tests en rojo en TS: `bucketing` usa `data.bucket` (día y mes) y `dayPoints` rellena huecos sin reagrupar; el mock devuelve `bucket` con la misma regla (399/400/401 días); verificar que fallan; implementar en `panels.tsx` y `mock.ts`; verificar `npm test`, `npm run build`, el aspecto con «Todo», y medir S2 con «Todo» aplicando la regla (si se revierte, revertir 7.2 también)

## 8. Cierre de la fase

- [ ] 8.1 Mutación dirigida (D9) sobre `query.ts`, `filterStore.ts`, `virtual.ts`, las derivaciones de 6.1 y, si se conservó, `snapshot.rs` (`bucket_for`, `rebucket_month`); cada ejecución cortada a los 3 min; verificar > 95 % o supervivientes justificados en design.md
- [ ] 8.2 Medición final con el mismo arnés que 1.3 (104 y 630 ms) y verificar los presupuestos de `specs/rendimiento` («Presupuestos de interacción»); añadir a `docs/perf/BASELINE.md` la sección «Después de la Fase 4» (antes/después de S1–S4, render de React, chunks bytes/gzip, pasos conservados y revertidos, reproducción con `node scripts/perf-ui.mjs`, seguimientos) y documentar la caché y el almacén de filtro en `docs/wiki/Arquitectura.md`
- [ ] 8.3 Comprobaciones finales: `npm test` y `npm run build` en verde; si hubo cambios en Rust, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo test --workspace` en verde; `package.json` sin dependencias nuevas; `npm run dev` con el mock recorre todos los apartados igual que en la partida

## Workflow follow-up

- Archivar el cambio (`/opsx:archive`) tras la revisión.
