# Mutation testing

El diff solo toca `src/styles.css` y tests. El proyecto no tiene herramienta de mutación para CSS (Stryker muta JS/TS, y este cambio no modifica TypeScript de producción), así que no se añade una dependencia: se aplicaron mutantes a mano sobre las reglas del diff, uno a uno, ejecutando `npx vitest run src/components/__tests__/styles-grafito.test.ts` y restaurando el CSS después de cada uno.

**Resultado: 62/62 mutantes muertos (100 %) en 56 s.**

| Grupo | Mutantes |
|---|---|
| Paleta | `--bg` oscuro, `--sidebar` claro, `--surface-2` oscuro, `--row-border` claro, `--track` oscuro, `--text-muted` oscuro y claro (vuelta a los tonos de la maqueta que no llegan a AA) |
| `--card-edge` | valor oscuro y claro |
| Colores de series (golden) | `--agent-claude` alterado en un dígito |
| Tarjeta | `border-radius: 12px`, `background: var(--surface-1)`, sin `position: relative`, `border: 1px solid`, con sombra, regla solo para `.panel`, `::before` solo para `.kpi` |
| `::before` | sin `content`, `position: relative`, `inset: 1px`, `padding: 2px`, `pointer-events: auto`, ángulo `90deg`, arranque `var(--accent)`, `transparent 50%`, `transparent 60%`, final `var(--row-border)`, `-webkit-mask` y `mask` con `padding-box`, `-webkit-mask-composite: source-over`, `mask-composite: add` |
| Compensación de 1 px | padding de `.panel`, `.kpi`, `.kpis-compact .kpi` y `.panel` en `@media (max-width: 860px)` |
| Líneas de 0,5 px | `.table-head`, `.table-row`, `.panel-note` a `1px` |
| Elementos rectos | quitar cada selector de la regla de radio 0 (16 mutantes); radio y sombra en `.segment.active`; sombra en `.settings-tab.active`; reintroducir los radios de `.share-bar > .share-seg:first-child/last-child/only-child` |
| Controles de formulario | `--control-radius: 0`; `.button` con `border-radius: 0` |
