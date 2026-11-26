# Design

## Context

- Todo el estilo vive en `src/styles.css`: tokens en `:root` (oscuro por defecto) y `:root[data-theme="light"]`; el selector de tema solo cambia el atributo `data-theme`, así que basta con cambiar tokens y reglas para que ambos modos (y «sistema») reciban el rediseño.
- `.panel` y `.kpi` hoy tienen `background: var(--surface-1)`, `border: 1px solid var(--border)` y `border-radius: 12px`; ninguno usa `::before`/`::after` ni `position`.
- Ya existe un patrón de test que lee `styles.css` como texto con `readFileSync` (`src/components/__tests__/charts.test.ts`) y un patrón golden con `UPDATE_GOLDEN=1` (`DataTable.test.tsx`, carpeta `__golden__/`).
- Referencia visual: `docs/design/propuesta-temas-cards.html`, tema Grafito, línea «E · Degradado».
- Requisitos: ver `specs/interfaz/spec.md`.

## Goals / Non-Goals

**Goals:**
- Cambio solo en CSS (tokens + reglas), sin tocar componentes React.
- Proteger con tests automáticos los tokens nuevos, la forma de las tarjetas y la invariancia de los colores de series.

**Non-Goals:**
- Paneles flotantes (`.popover`, `.select-list`, `.dialog`, `.palette`): no son tarjetas; conservan relleno, radio y sombra porque deben tapar lo que hay debajo. Solo heredan la paleta nueva por tokens.
- Radio de botones, botones de filtro, desplegables, campos y buscador (`--control-radius` se mantiene en 8 px).
- Tipografía, espaciados y estructura de paneles.

## Decisions

1. **Paleta por tokens existentes.** Se reasignan `--bg`, `--sidebar`, `--surface-1..3`, `--border`, `--row-border`, `--text-*` y `--track` en ambos bloques. `--sidebar` toma el valor de `--bg` (oscuro `#0f0f10`, claro `#f4f4f5`), siguiendo la maqueta, donde el lateral no se distingue del fondo; la separación la da su borde derecho. Alternativa descartada: crear tokens nuevos por componente (más superficie, mismo resultado).
2. **Texto atenuado ajustado a AA.** Contraste medido (WCAG 2.x): `#77777d` sobre `#0f0f10` = 4.31:1 y `#8a8a92` sobre `#f4f4f5` = 3.12:1; ninguno llega a 4.5:1. Se usan `#848489` en oscuro (5.15:1 sobre el fondo, 4.57:1 sobre `#1c1c1f`) y `#6b6b73` en claro (4.81:1 sobre el fondo, 5.28:1 sobre blanco, 4.64:1 sobre `#f0f0f2`), los tonos más cercanos al original que cumplen también sobre las superficies de *hover*.
3. **Token `--card-edge`.** Valor `#f2a33a` (oscuro) y `#20b0f0` (claro). Se declara con el literal y no como `var(--accent-soft)`, para que el borde no cambie si en el futuro se toca el acento de controles, y para que el test pueda comprobar el valor exacto.
4. **Borde degradado con pseudo-elemento enmascarado**, idéntico a la maqueta:
   - Tarjeta: `position: relative; background: transparent; border: 0; border-radius: 0; box-shadow: none;`.
   - `::before`: `content: ""; position: absolute; inset: 0; padding: 1px; pointer-events: none; background: linear-gradient(135deg, var(--card-edge), transparent 40%, transparent 65%, var(--border)); -webkit-mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0); -webkit-mask-composite: xor; mask-composite: exclude;` (más `mask` estándar con los mismos valores).
   - Alternativas descartadas: `border-image` (no admite transparencia intermedia limpia con `border-radius` y no permite reutilizar el gris de borde al final de forma legible); `background-clip` doble (exige relleno opaco, y las tarjetas deben ser transparentes).
   - Al quitar `border: 1px` el contenido se desplaza 1 px; se compensa añadiendo 1 px al `padding` de `.panel`, `.kpi` y `.kpis-compact .kpi` para que la caja mida lo mismo.
5. **Qué es «tarjeta».** Reciben el borde degradado: `.panel` y `.kpi` (incluida la variante `.kpis-compact`). Reciben solo radio 0 (sin degradado, para no anidar bordes de acento dentro de un panel): `.notice`, `.today-session`, `.theme-card`.
6. **Líneas de 0,5 px.** `.table-head` (`border-bottom`), `.table-row` (`border-bottom`) y `.panel-note` (`border-top`) pasan de `1px` a `0.5px`. En pantallas de densidad 1 algunos navegadores redondean a 1 px; se acepta (ver riesgos).
7. **Radio 0 en elementos internos.** `.chip`, `.segmented`, `.segment` (incluido `.segment.active`), `.bar-track`, `.bar-fill`, `.inline-bar`, `.inline-bar-fill`, los segmentos de `.share-bar` y `.card-alert`. `.segment.active` pierde también su sombra. Las columnas del gráfico de gasto (`.column-*`) y los puntos de leyenda son marcas de gráfico, no medidores: no se tocan.
8. **Spec «Consistencia de tamaños».** Exigía radio común para todos los controles, incluidos los segmentados; se modifica para que los segmentados sean rectos y el resto conserve `--control-radius`.
9. **Estrategia de test** (nuevo `src/components/__tests__/styles-grafito.test.ts`, estilo de `charts.test.ts`):
   - Helper que extrae el bloque de declaraciones de un selector exacto (`:root`, `:root[data-theme="light"]`, `.panel`…) y devuelve un mapa propiedad → valor; si un selector aparece varias veces, la última declaración gana, como en la cascada.
   - Tokens: valores exactos de la paleta por modo (tabla de casos por partición oscuro/claro) y `--card-edge`.
   - Contraste: función de luminancia relativa WCAG en el test; `--text-muted` vs `--bg` ≥ 4.5 en ambos modos (valor límite: el original `#77777d` debe dar < 4.5, caso de *error guessing* que valida la propia función).
   - Forma: `border-radius: 0` y `background: transparent` en `.panel`/`.kpi`; `::before` con `pointer-events: none`, `padding: 1px`, `linear-gradient(135deg, var(--card-edge)`, `transparent 40%`, `transparent 65%`, `var(--border)`, `mask-composite: exclude` y `-webkit-mask-composite: xor`.
   - Grosor 0,5 px en las tres líneas internas; radio 0 en la lista de la decisión 7; `--control-radius` sigue en `8px` en `.button`/`.select`.
   - Colores de series: **golden file** `__golden__/series-tokens.json` con todos los `--palette-*`, `--series-*`, `--act-*`, `--agent-*` de ambos bloques, generado con `UPDATE_GOLDEN=1` **antes** de tocar el CSS y comparado después.

## Risks / Trade-offs

- [`mask-composite` no soportado] → WebKitGTK (Tauri en Linux) y WebView2 soportan `-webkit-mask-composite: xor`; se declaran ambas variantes. Si fallara, la tarjeta se vería con el degradado relleno: se verifica a mano en `npm run tauri dev` en ambos modos.
- [Líneas de 0,5 px redondeadas a 1 px o invisibles en pantallas de densidad 1] → Se acepta; el color `--row-border` es casi invisible en ambos casos.
- [Tarjetas transparentes reducen la separación visual] → La aporta el borde degradado; validado en la maqueta.
- [Tests sobre texto CSS son frágiles ante reformateos] → El helper normaliza espacios y busca por selector exacto, no por número de línea.
- [Mutation testing] → No hay herramienta de mutación para CSS en el proyecto; el cambio no modifica TypeScript de producción. Se justifica en la tarea correspondiente en vez de añadir una dependencia nueva.

## Migration Plan

- Cambio puramente visual, sin datos ni preferencias persistidas: no requiere migración. Reversión con `git revert` del commit de estilos.
