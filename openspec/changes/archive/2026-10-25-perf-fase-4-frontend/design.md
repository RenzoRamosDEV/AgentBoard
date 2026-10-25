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

### Partida

Commit de partida `c732391` (arnés sin cambios de rendimiento). `node scripts/perf-ui.mjs --latency=104,630` (3 ejecuciones × 5 repeticiones; mediana / máximo en ms; render = suma de `actualDuration` del `Profiler` por interacción, mediana). Los tiempos van en múltiplos de ~16,7 ms (fotogramas de Chromium headless a 60 Hz). Cifras con el arnés final (ver «Arnés»), medidas en un worktree de la partida:

| Escenario | 104 ms | render | 630 ms | render |
|---|---|---|---|---|
| S1 frío (Resumen → Modelos, primera vez) | 33,2 / 33,4 | 10,6 | 33,2 / 33,3 | 10,5 |
| S1 cacheado (idas y vueltas siguientes) | 66,5 / 67,2 | 9,6 | 199,9 / 200,5 | 10,3 |
| S2 frío (30 d → 7 d tras un aviso) | 166,8 / 183,7 | 20,6 | 833,3 / 833,5 | 20,7 |
| S2 cacheado (7 d → 30 d) | 166,7 / 167,7 | 15,7 | 816,6 / 818,7 | 16,0 |
| S3 frío (ocultar un agente) | 216,7 / 217,3 | 26,1 | 999,8 / 1000,2 | 25,6 |
| S4 aviso con el Resumen abierto | 183,3 / 183,5 | 25,4 | 833,4 / 833,6 | 24,1 |
| S5 «Todo» (frío; mock sin historial largo en la partida) | 166,9 / 183,7 | 22,0 | 833,2 / 838,2 | 21,3 |

S4, peticiones por aviso: `get_dashboard_snapshot` ×2, `get_summary` ×3, `get_timeseries`, `list_sessions`, `list_agents`, `list_projects` (9). El Resumen se pedía dos veces porque las listas de agentes y proyectos llegaban como objetos nuevos y cambiaban el `filter`. S1 cacheado con 630 ms incluye la recarga de «Hoy en directo» al volver al Resumen.

Bundle (build normal): un único `index-*.js` de 401 405 B (115 448 B gzip) y `index-*.css` de 48 697 B (9 191 B gzip).

### Arnés

- `--dump-dom` no espera al `await` de nivel de módulo (el `load` llega antes): `scripts/perf-ui.mjs` usa el protocolo de DevTools con el `WebSocket` de Node 24 (sin dependencias) y espera a `<pre id="perf-results">`.
- Las funciones puras del arnés están en `src/dev/perfStats.ts` (las importa también el script con la eliminación de tipos de Node 24); `chunkSizes` e `initialChunks` (chunks del arranque según `index.html`) en el script, con su `.d.mts`.
- El fin de cada interacción es el segundo `requestAnimationFrame` tras el último cambio del DOM fuera de la barra superior y del panel lateral (el reloj «hace N s» repinta la barra cada segundo), o tras el render que sigue a la última respuesta, con 200 ms sin peticiones ni cambios. Primera versión: la espera contaba desde la actividad anterior y no desde el clic, así que una transición o un chunk perezoso que pintaban unos fotogramas después quedaban fuera (S1 frío daba 0 ms con `React.lazy`). Se corrigió (`c5633da`) y se volvieron a medir la partida y los pasos 2–4 con el arnés corregido; las cifras de esta sección son esas.
- La build `perf` usa `react-dom/profiling` (el `Profiler` no mide en producción), se escribe en `node_modules/.perf-dist` y no toca `dist/`; `npm run build` no contiene `installMocks` ni el arnés.
- Añadido S5 («Todo» con `?history=1095` en el mock) para medir D8; sin `history`, el mock devuelve lo mismo que antes.
- `data-perf` en los botones del panel lateral, del periodo, en el botón de Agentes (prop `perf` de `Popover`) y en las casillas de agente (S3).

### Cifras por paso (104 ms, mediana; render entre paréntesis)

| Paso | S1 frío | S1 cach. | S2 frío | S2 cach. | S3 | S4 | Arranque B / gzip | Decisión |
|---|---|---|---|---|---|---|---|---|
| Partida | 33,2 | 66,5 | 166,8 (20,6) | 166,7 | 216,7 (26,1) | 183,3 (25,4) | 450 102 / 124 639 | — |
| D2/D3 caché (`f309088`) | 33,4 | 33,3 | 133,4 (13,9) | 33,4 | 133,5 (25,2) | 133,3 (41,1) | 451 291 / 125 381 | conservado (−20 a −80 %) |
| D4 filtro (`b947e61`) | 33,4 | 33,4 | 133,5 (8,3) | 33,4 | 133,4 (18,7) | 133,3 (9,8) | 453 488 / 126 097 | conservado (render −26 a −76 %, estructural) |
| D5 lazy (`8da7f45`) | 33,5 | 33,4 | 133,5 (9,1) | 33,5 | 133,3 (18,3) | 133,3 (10,1) | 397 489 / 116 871 | conservado (estructural, arranque −12 %) |
| D6 tablas (`43c03fe`) | 33,1 | 33,4 | 133,4 (9,0) | 33,4 | 133,4 (17,2) | 133,3 (10,1) | 399 678 / 117 778 | conservado (estructural, sin empeorar) |
| D7 memos (`ae84275`) | 33,3 | 33,3 (8,8) | 133,4 (9,2) | 33,3 | 133,4 (17,5) | 133,4 (9,8) | 401 856 / 118 458 | **revertido** (`c9f91ac`): render S1 −6 %, resto igual |
| D8 cubo mensual | — | — | — | — | — | — | — | **no aplicado** (ver abajo) |
| Final (`b4b30d9`) | 33,1 | 33,3 | 133,3 (8,5) | 33,3 | 133,4 (18,6) | 133,4 (10,2) | 399 658 / 117 786 | S5 133,4 (18,6) |

### Decisiones y desviaciones

- **D2**: `useQuery(cmd, key, fetcher)` en lugar de `useQuery(cmd, args, key)`: el Resumen no es un `invoke` directo (`loadDashboard` añade los filtros) y «Hoy en directo» combina cinco peticiones. `setGeneration(g)` siempre avanza (`max(g, actual + 1)`): `ingest://done` sube la generación local y un `data:changed` posterior puede traer el mismo número. Una entrada con error no queda «en curso»: el siguiente suscriptor reintenta.
- **D3**: `subscribeApp(listen, store, { changed, live })`; `changed` solo actualiza «hace N s». `TrayPanel` (ventana aparte con su propia caché) llama a `invalidateAll` con cada aviso para que «Hoy en directo» recargue como antes: cambio mínimo, necesario al quitar `refresh` de `TodayLive`. «Hoy en directo» usa la clave `agentes|proyectos|tick` y conserva en pantalla la carga anterior mientras llega la del tick siguiente (como antes, sin «Cargando…» cada 30 s). La clave del Resumen es `dashboardKey(filtro, proyecto único, anterior, inicio del mes)`.
- **D4**: los cambios de filtro van por `useSyncExternalStore`, cuyas actualizaciones son siempre síncronas: `startTransition` no les afecta, así que la transición del filtro la da `useDeferredValue` en el contenido (el cambio de apartado sí va en `startTransition`). Para no perder un fotograma en frío, el Resumen del filtro nuevo se pide en el render urgente y se pinta con el diferido. `Overview`, `Section` y `Projects` se memoizan en `App`; `Sidebar` lee sus cifras del pie (agentes y sesiones) de la caché. Exportar lee el último filtro derivado (`filterStore.current()`), sin suscribir la app.
- **D5**: el arranque en bytes baja un 11–12 %, pero en gzip sube un 2 % (117 786 frente a 115 448 + 9 191 B): varios chunks pequeños comprimen peor que uno. El `index-*.js` sí es mucho menor (231 560 B).
- **D6**: el orden local se memoiza con una caché por lista (`WeakMap` por `rows`, función de orden y sentido) en lugar de `useMemo`: así se puede probar con `renderToStaticMarkup` (no hay renderer de cliente en los tests) y es equivalente. Los espaciadores necesitan `.table-spacer { flex-shrink: 0 }` (la tabla es un `flex` en columna). `aria-rowcount` cuenta las filas de datos (1 000) y `aria-rowindex` empieza en 1, como pide la tarea 5.2. Antes de medir, la altura supuesta de fila es 32 px y la ventana 900 px. Columnas memoizadas en `Projects`, `ProjectDetail`, `SessionDetail` y las vistas ampliadas de desgloses.
- **D7**: revertido por la regla. El golden «de sus salidas» se grabó como HTML de `Overview` y de todos los apartados sobre datos deterministas de 30 y 450 días (`src/views/__tests__/__golden__/derivaciones.json`, antes de tocar nada), porque las funciones puras no existían con el código de partida; las pruebas de `rankItems`/`rankKeys`/`stacksByBucket` se fueron con el paso revertido. Se conserva el test del invariante de animaciones (`src/components/__tests__/charts.test.ts`), que ya se cumplía. `DailyPanel` y la vista ampliada de `ActivityPanel` no se usan en ninguna vista.
- **D8 no aplicado** (7.1–7.3 sin hacer): con tres años en «Todo» (S5), toda la interacción tarda 133,4 ms con 18,6 ms de render. Renderizar el Resumen con las series ya agrupadas por mes ahorra ~5,5 ms (19,7 → 14,3 ms por `renderToStaticMarkup` del Resumen de 1 095 días), un 4 % y menos de un fotograma: el paso no podía cumplir la regla del 10 %, que pedía revertirlo entero (Rust, TS y mock). Se midió antes de escribir el código para no tocar `snapshot.rs` en vano. Queda como seguimiento medir con el backend real el coste de serializar por IPC años de series diarias, que el mock no refleja.
- **D9**: Stryker 9 (`npx`, con `typescript@5` añadido porque su preprocesador lo importa) arranca con vitest 5, pero no activa los mutantes (cada mutante ejecuta solo 2 pruebas de `DataTable` y sobreviven mutantes obvios, p. ej. `shouldVirtualize = () => true`, que rompe el golden de 200 filas). Se usó un mutador de operadores propio sobre el diff (comparaciones, lógicos, `??`, `true/false`, `Math.min/max/floor/ceil`, `+/-`, `++/--`, `.size`, `.delete`), un mutante cada vez con `vitest --bail=1` sobre los tests del módulo y 3 min por archivo. Sin cambios en Rust, no hubo `cargo mutants`.

### Mutación

| Archivo | Mutantes | Muertos | Supervivientes |
|---|---|---|---|
| `src/lib/virtual.ts` | 13 | 12 | 1 |
| `src/lib/query.ts` | 43 | 41 | 2 |
| `src/lib/filterStore.ts` | 39 | 35 | 4 |
| Total | 95 | 88 (92,6 %) | 7, todos equivalentes |

La primera pasada dejó 9 supervivientes en `query.ts` y 6 en `filterStore.ts`; se añadieron pruebas (respuesta que sustituye a la anterior al caducar en la misma generación, error en la misma generación y reintento, límite exacto del LRU, orden LRU por uso y no por llegada, proyecto único sin ocultos) y se extrajo `visibleAgents`, compartida por `derive` y `useLists`. Supervivientes, equivalentes:

- `virtual.ts`: `count <= 0` → `count < 0`: con 0 filas el recorte a `[0, count]` da igualmente `{0, 0}`.
- `query.ts`: `entries.size <= maxEntries` → `<` en la salida temprana de `evict`: con el tamaño igual al límite, el bucle sale en la primera comprobación sin descartar nada. `snap ?? EMPTY` → `||`: la instantánea es siempre un objeto.
- `filterStore.ts`: `typeof localStorage === "undefined"` en el almacén del módulo (solo elige el almacenamiento del navegador; las pruebas usan almacenamientos falsos); `data ?? []` → `||` (×2) y `error ?? error` → `||` (la consulta da un array o `undefined`, y un error es un texto no vacío o `null`).
