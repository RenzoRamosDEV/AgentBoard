# Proposal

## Why

Tras la Fase 3 el backend ya entrega el Resumen en una petición (~104 ms en frío sobre 200 MB) y avisa con `data:changed` como mucho una vez por segundo, pero la interfaz sigue volviendo a pedir y a calcular todo en cada cambio: cada vista guarda sus datos en estado local, el contador `refresh` invalida todas las consultas (también las de vistas que no se ven), el filtro vive en `App` y re-renderiza la app entera, las tablas pintan todas sus filas y los gráficos reagrupan las series por día o mes en cada render. El objetivo de esta fase es que un cambio de filtro o de apartado se perciba en menos de 100 ms con datos ya cargados.

## What Changes

- Caché de datos de la interfaz por `[comando, filtro, generación]`, con un hook propio (sin `@tanstack/react-query`): sin refetch al volver el foco, `staleTime` alto y, al llegar `data:changed`, solo se vuelven a pedir las consultas de las vistas montadas; las demás quedan marcadas como obsoletas y se piden al montarse, mostrando mientras tanto los datos anteriores.
- Estado de filtro global ligero (periodo, agentes y proyectos ocultos, comparación) en un almacén externo con suscripción por selector; la barra de filtros y las vistas se suscriben a lo que usan. Los cambios de filtro y de apartado se marcan como transición (`startTransition`/`useDeferredValue`).
- `React.lazy` por apartado (Resumen y su vista ampliada, Proyectos, detalle de proyecto y de sesión, Precios, Ajustes, buscador).
- Tablas: virtualización propia (sin `@tanstack/react-virtual`) a partir de 200 filas; filas memoizadas; el orden local de `DataTable` y las agrupaciones de los paneles se calculan una vez por dato, no en cada render.
- Gráficos: las series de puntos se memoizan por dato y métrica; ningún gráfico anima sus cambios de geometría (invariante, hoy ya se cumple, que se protege con un test).
- ~~Backend: el Resumen en una petición decide el cubo temporal (día o mes) y devuelve las series ya agrupadas en ese cubo.~~ No aplicado: medido, no podía mejorar un 10 % (design.md, «D8 no aplicado»); el requisito «Resumen en una sola petición» queda como estaba.
- Medición: arnés sin dependencias nuevas que abre la build de producción con el mock en Chromium headless y mide cambio de apartado y de filtro (frío y cacheado) con React Profiler y `performance.now`; tamaño del bundle con el informe de `vite build`. Antes/después en `docs/perf/BASELINE.md` («Después de la Fase 4»). Cada paso que no mejore se revierte.
- Sin cambios visuales: mismo diseño aprobado, mismas cifras y mismos textos.

## Capabilities

### New Capabilities
<!-- ninguna -->

### Modified Capabilities
- `dashboard`: la actualización tras la importación refresca solo los apartados visibles y reutiliza datos ya cargados.
- `rendimiento`: requisitos de la interfaz (caché de datos, tablas largas virtualizadas, carga por apartado, presupuestos de interacción medidos y comparación antes/después).

## Impact

- Frontend: `src/App.tsx`, `src/lib/useData.ts`, `src/lib/live.ts`, nuevos módulos en `src/lib/` (caché de consultas, almacén de filtro, rango visible), `src/components/DataTable.tsx`, `Charts.tsx`, `Topbar.tsx`, `Sidebar.tsx`, `src/views/*` (Overview, Section, sections, panels, Projects, ProjectDetail, SessionDetail, Pricing, TodayLive), `src/dev/mock.ts`, `src/main.tsx` (modo de medición), `vite.config.ts` (modo `perf`).
- Backend: sin cambios (D8 no aplicado).
- Dependencias: ninguna nueva en `package.json`; la mutación de TS usa `npx` puntual.
- Medición y documentación: `scripts/perf-ui.mjs`, `docs/perf/BASELINE.md`.
