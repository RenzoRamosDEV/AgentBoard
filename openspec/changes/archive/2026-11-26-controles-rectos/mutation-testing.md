# Mutation testing

El diff solo toca `src/styles.css` y tests; no hay herramienta de mutación para CSS, así que se aplicaron mutantes a mano sobre las reglas del diff, uno a uno, ejecutando `npx vitest run src/components/__tests__/styles-grafito.test.ts` y restaurando el CSS después de cada uno.

**Resultado: 43/43 mutantes muertos (100 %) en 48 s.**

| Grupo | Mutantes |
|---|---|
| Token | `--control-radius` de vuelta a `8px` |
| Regla agrupada de radio 0 | quitar del grupo cada uno de los 26 selectores nuevos (`kbd`, `.brand-version`, `.nav-new`, `.badge`, `.tag`, `.src-tag`, `.compare-pill`, `.live`, `.nav-item`, `.sidebar-toggle`, `.menu-item`, `.select-option`, `.palette-item`, `.price-input`, `.range input`, `.checklist li`, `.today-session-row`, `.theme-preview`, `:focus-visible`, `.switch`, `.switch::after`, `.popover`, `.dialog`, `.palette`, `.select-list`, `.tooltip`); poner el grupo a `2px` |
| Controles | sustituir `var(--control-radius)` por `8px` en `.button`, `.select`, `.chip-button`, `.search`, `.field input`, `.budget-field` y `.scoped-add input` |
| Círculos intactos | `.brand-logo`, `.live-dot` y `.status-dot` con radio 0 |
| Sombras de flotantes | quitar la sombra de `.popover`, `.palette`, `.tooltip` y `.select-list` |
| Interruptor | pulgar de 13 px de ancho |

En una primera pasada sobrevivió «quitar la sombra de `.select-list`»: la regla base repetía una sombra (y un padding) que la regla posterior sobrescribía. Se eliminaron esas declaraciones muertas de la regla base (sin cambio de estilo calculado) y el mutante pasó a morir.
