# Design

## Context

Motivación en proposal.md («Why»); requisitos en `specs/dashboard` y `specs/rendimiento`. Estado observado:

- `App.tsx` guarda periodo, agentes/proyectos ocultos, comparación, apartado y un contador `refresh` que sube con cada `data:changed`/`ingest://done` (`subscribeApp`) y con `saveSettings`. Todo cambio re-renderiza la app entera; `range` depende de `refresh`, así que cada aviso recalcula `filter` y vuelve a pedir agentes, proyectos, `summary({})` y el Resumen.
- Cada vista (`Projects`, `ProjectDetail`, `SessionDetail`, `Pricing`, `TodayLive`, `CommandPalette`) pide con `useEffect` + `useState` y depende de `refresh`; no hay caché entre vistas ni al volver a un filtro.
- `useDashboardData` → un `invoke("get_dashboard_snapshot")` (Fase 3).
- `DataTable` ordena con `[...rows].sort` + `localeCompare` en cada render (modo no controlado) y pinta todas las filas visibles; se pliega a 20 filas, pero al desplegar o con «Ver más» (sesiones de un proyecto, turnos) puede superar 200.
- Agrupaciones en render, todas recalculadas en cada render del panel: `bucketing`/`dayPoints` (`panels.tsx`), pilas por día de `DailyPanel`/`DailyFull` (`panels.tsx` ~150–170), `ActivityPanel` ampliado (~420–435), `Overview` (gráfico por agente, ~50–70), `EvolutionCard` (ranking de claves y pilas, `sections.tsx` ~160–185), `ActivityFull` (~280–290), `RankingCard` (`sections.tsx` ~67), KPIs de `Section.tsx` (`[...data.daily].sort`, `[...rows].sort`), `projectStats`/`sessionStats` (ya en `useMemo`).
- Gráficos (`Charts.tsx`) dibujados a mano en SVG; ninguna regla CSS anima su geometría (las únicas transiciones son de controles: barra lateral, interruptores, `share-seg` con `filter`).
- No hay `JSON.stringify` en render (solo en `mock.ts` al guardar ajustes).
- Tests de vitest en entorno `node` (sin jsdom ni Testing Library): funciones puras y `renderToStaticMarkup`.
- El mock solo se instala en `DEV`; no hay forma de medir la build de producción con datos.

## Goals / Non-Goals

**Goals:** presupuestos de `specs/rendimiento` («Presupuestos de interacción») medidos de forma reproducible y sin dependencias nuevas; misma interfaz pixel a pixel.

**Non-Goals:** rediseño; cambios en `TrayPanel` (ventana aparte, ya sin intervalos); contención con lecturas largas y checkpoint WAL diferido (pendiente para la entrega final); compilaciones `release` de Rust (D8 solo necesita tests).

## Decisions

### D1. Medición: arnés propio en Chromium headless y regla de conservar/revertir

- `vite build --mode perf` (`.env.perf` con `VITE_MOCK=1`): `main.tsx` instala el mock también si `import.meta.env.VITE_MOCK === "1"`; React sigue en producción. El mock acepta `?latency=<ms>` que retrasa `get_dashboard_snapshot` (resto de comandos: 1/4 de esa latencia).
- `?perf=1` monta `src/dev/perfHarness.ts` (import dinámico, solo en modo `perf`): envuelve `App` en `<Profiler>`, ejecuta los escenarios con clics reales sobre el DOM (`data-perf` en los botones del panel lateral y del selector de periodo; atributo sin efecto visual) y mide desde `performance.now()` justo antes del clic hasta el segundo `requestAnimationFrame` tras ver el resultado (encabezado del apartado y ningún «Cargando…»). Escribe el JSON en `<pre id="perf-results">` antes de que termine `start()`, así `load` espera al arnés.
- `scripts/perf-ui.mjs` (Node 24, sin dependencias): `npx vite build --mode perf`, `npx vite preview --port 5199`, y `chrome-headless-shell --headless --dump-dom "http://localhost:5199/?perf=1&latency=104"` (ruta por glob `~/.cache/ms-playwright/chromium_headless_shell-*/*/chrome-headless-shell`, o `CHROME` en el entorno); extrae el JSON, repite la ejecución 3 veces y da mediana y máximo. Imprime además el tamaño de cada chunk de `dist/assets` (bytes y gzip con `node:zlib`).
- Escenarios (5 repeticiones cada uno): S1 apartado Resumen → Modelos → Resumen (cacheado tras la primera); S2 periodo 30 d → 7 d (frío) y 7 d → 30 d (cacheado); S3 ocultar un agente (frío); S4 `data:changed` simulado con el Resumen abierto: peticiones hechas (solo las de la vista montada) y tiempo de render. Latencias: 104 ms (Resumen de 200 MB en frío, «Después de la Fase 3»); informativo con 630 ms (1 GB).
- Bundle: chunks antes/después del informe de `vite build` (ya lo imprime; sin `rollup-plugin-visualizer`).
- Regla: cada paso de rendimiento (D3–D8) se conserva si mejora su escenario ≥ 10 % sin empeorar otro > 10 %, o si es estructural (lo exige un requisito) y no empeora > 10 %; si no, se revierte y se anota en «Desviaciones durante la implementación». Umbral del 10 % (no 5 % como en Rust) por el ruido de un navegador headless; mediana de 3 ejecuciones × 5 repeticiones.
- Alternativa descartada: Playwright como dependencia (pesada, solo para medir) y `--virtual-time-budget` (falsea `performance.now`).

### D2. Caché de consultas propia, sin `@tanstack/react-query`

`src/lib/query.ts`: `createQueryStore({ now, maxEntries = 64, staleMs = 10 min })` puro + `useQuery(cmd, args, key)` con `useSyncExternalStore`.
- Entrada por `cmd + "|" + key`; guarda `data`, `error`, `generation`, `fetchedAt`, `inflight`, `subscribers`. Es fresca si su generación es la actual y tiene menos de `staleMs` (cubre el cambio de día en periodos relativos). Equivale a la clave `[comando, filtro, generación]`, pero al cambiar la generación se conserva el dato anterior para pintarlo mientras llega el nuevo.
- `setGeneration(g)` (desde `data:changed` e `ingest://done`) y `invalidateAll()` (al guardar ajustes): vuelven a pedir solo las entradas con suscriptores; el resto se pide al suscribirse.
- Peticiones iguales simultáneas comparten la promesa; una respuesta de una generación ya superada no pisa una más nueva.
- LRU por último uso entre entradas sin suscriptores.
- Sin escucha de `focus`/`visibilitychange` (no hay refetch al recuperar el foco).
- `key` la da el llamante con `filterKey(filter)` (campos en orden fijo unidos con `|`), calculada en `useMemo` del almacén de filtro (D4): ningún `JSON.stringify` en render.
- Justificación: los casos (clave, generación, dedupe, montadas) caben en ~120 líneas probadas; React Query añade ~13 KB gzip y su modelo de invalidación por foco/intervalos hay que desactivarlo entero.

### D3. Avisos de datos → generación

`subscribeApp` deja de sumar `refresh`: llama a `store.setGeneration(payload.generation)` (con `ingest://done`, generación local siguiente). El resalte en vivo no cambia. Las vistas dejan de recibir `refresh`; `ProjectDetail` y `SessionDetail` reinician su `pager` cuando cambia la generación (hook `useGeneration()`), como hoy con `refresh`. `TodayLive` sigue con `whenVisible`.

### D4. Almacén de filtro ligero

`src/lib/filterStore.ts`: almacén externo (`useSyncExternalStore`) con `period`, `hiddenAgents`, `hiddenProjects`, `compare` (y su `localStorage`), y selectores `useFilterState(sel)`; `useFilter()` devuelve el `Filter` memoizado y su `filterKey`. `Topbar` y las vistas se suscriben a lo que usan; `App` solo guarda apartado/proyecto/sesión y ajustes. Los cambios de filtro y de apartado van dentro de `startTransition`; las vistas pesadas usan `useDeferredValue(filter)` para que la barra responda al instante y el contenido se pinte en segundo plano. `Sidebar` y `Topbar` con `memo`.
Alternativa descartada: Context (re-renderiza a todos los consumidores) o Zustand (dependencia para ~40 líneas).

### D5. `React.lazy` por apartado

Lazy: `Section`, `Projects`, `ProjectDetail`, `SessionDetail`, `Pricing`, `SettingsDialog`, `CommandPalette`. `Overview` sigue en el chunk inicial (primera pantalla). `Suspense` con el mismo `<div className="main muted">Cargando…</div>` de hoy (los diálogos, con `fallback={null}`). Precarga con `import()` al pasar el ratón por el elemento del panel lateral: no hace falta salvo que S1 en frío supere 300 ms.

### D6. Tablas: orden memoizado, filas memoizadas y virtualización propia

- `DataTable`: orden local en `useMemo([rows, sort, columns])`; fila extraída a `TableRow = memo(...)` comparando `row`, `columns`, `template` y manejadores; las vistas con tablas que pueden superar 200 filas (`Projects`, sesiones de `ProjectDetail`, turnos de `SessionDetail`, vistas ampliadas de desgloses) definen `columns` con `useMemo`.
- Virtualización: `src/lib/virtual.ts` con `visibleRange({ scrollTop, viewport, offsetTop, rowHeight, count, overscan })` puro, y en `DataTable` un hook que, solo con > 200 filas mostradas, mide la altura de la primera fila (una vez, `ResizeObserver` sobre ella), escucha el `scroll` del contenedor desplazable más cercano (`passive`, agrupado por `requestAnimationFrame`) y pinta `[inicio, fin)` entre dos espaciadores de altura fija. Filas con `aria-rowindex` y la tabla con `aria-rowcount`. Con ≤ 200 filas el marcado es idéntico al de hoy.
- Justificación frente a `@tanstack/react-virtual` (~5 KB gzip): filas de altura fija (`white-space: nowrap`) y desplazamiento del contenedor; el caso cabe en una función pura de ~20 líneas.

### D7. Agrupaciones fuera del render y gráficos memoizados

- `RankingCard`, `EvolutionCard`, `ActivityFull`, `DailyPanel`/`DailyFull`, `ActivityPanel` ampliado, gráfico por agente de `Overview` y KPIs de `Section`: cada derivación (totales por clave, ranking, pilas por cubo, puntos) en `useMemo` por `[data, métrica, split]`; los `tooltip` se construyen dentro de esa memo. Los componentes de `Charts.tsx` con `memo`.
- Ordenar las pilas de cada punto (≤ `maxKeys` + 1 elementos) se queda en JS dentro de la memo: moverlo a Rust no ahorra nada medible.
- Invariante de animaciones: test que comprueba que ninguna regla de `styles.css` sobre clases de gráficos tiene `transition`/`animation` sobre geometría y que `Charts.tsx` no usa `<animate>` ni `style.transition`.

### D8. Cubo temporal en Rust (agrupación que hoy hace JS)

- `snapshot.rs`: `bucket_for(filter, first_ts, now, tz)` → `Day | Month` con la misma regla que `bucketing` en TS (más de 400 días entre el inicio y el final del periodo; sin `from`, desde el primer día con datos). Con `Month`, `rebucket_month` (pura) suma `daily`, `daily_by_*`, `activity_daily` y `prev.daily` por mes local y clave. Campo nuevo `bucket: "day" | "month"`. `month` y `hourly` no cambian.
- Con `Day` la respuesta es la de hoy (golden y `snapshot_resumen.rs` sin regrabar).
- TS: `bucketing` deja de calcular la regla y de reagrupar: usa `data.bucket`; `dayPoints` y las pilas solo rellenan huecos y leen el punto del cubo. El mock aplica la misma regla.
- Coste: sin compilación `release` (solo tests); en «Todo» con años de historial reduce los puntos que viajan y que JS reagrupa. Si S2 con «Todo» no mejora según D1, se revierte entero (Rust, TS y mock).

### D9. Mutación

- TS: `npx -y -p @stryker-mutator/core@9 -p @stryker-mutator/vitest-runner@9 stryker run` puntual (sin añadir a `package.json`), `--mutate` limitado a `query.ts`, `filterStore.ts`, `virtual.ts` y las funciones puras tocadas; corte a los 3 min.
- Rust: `cargo mutants --in-place --in-diff` sobre `snapshot.rs`, corte a los 3 min.
- > 95 % o cada superviviente justificado en este documento.

## Risks / Trade-offs

- [Buscar con Ctrl+F no encuentra filas no pintadas en tablas > 200] → solo afecta a listas desplegadas largas; el buscador de la app (Ctrl+K) no depende del DOM.
- [Altura de fila distinta (zoom, fuente)] → se mide en tiempo de ejecución y se vuelve a medir con `ResizeObserver`; el desfase máximo es una fila de margen (overscan 8).
- [Dato anterior visible tras un aviso] → es lo pedido (sin parpadeo de «Cargando…»); se sustituye en una petición.
- [Ruido del navegador headless en la medición] → umbral del 10 %, 3 × 5 repeticiones y misma máquina antes/después.
- [`useDeferredValue` deja un instante el contenido del filtro anterior] → aceptable: la barra responde y el contenido llega en el mismo presupuesto.
- [El cubo mensual en Rust con zona horaria distinta de la del navegador] → `tz` ya es `tzName()` del navegador; test con `Europe/Madrid` y cambio de horario.
- [Mutación TS con `npx` necesita red la primera vez] → si no hay red, se anota y se cubren los bordes con tests de tabla de valores.

## Migration Plan

Sin migración de datos ni de esquema. Orden: arnés y partida → D2/D3 → D4 → D5 → D6 → D7 → D8 → cierre. Cada paso es un commit; revertir es `git revert` del paso. El campo `bucket` es aditivo: una interfaz antigua lo ignora.

## Desviaciones durante la implementación

(se rellena al aplicar: partida, cifras por paso, pasos revertidos y supervivientes de mutación)
