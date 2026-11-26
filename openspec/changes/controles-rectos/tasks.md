# Tasks

## 1. Red

- [ ] 1.1 En `src/components/__tests__/styles-grafito.test.ts`, cambiar los tests que exigen `--control-radius: 8px` para exigir `0` y añadir tests de radio 0 para `kbd`, `.brand-version`, `.nav-new`, `.badge`, `.tag`, `.src-tag`, `.compare-pill`, `.live`, `.nav-item`, `.sidebar-toggle`, `.menu-item`, `.select-option`, `.palette-item`, `.price-input`, `.range input`, `.checklist li`, `.today-session-row`, `.theme-preview`, `:focus-visible`, `.switch`, `.switch::after` y los flotantes `.popover`, `.dialog`, `.palette`, `.select-list`, `.tooltip` (con sombra conservada); que los controles siguen usando `var(--control-radius)`; y que `.brand-logo`, `.live-dot` y `.status-dot` siguen en `50%`. Verificar con `npx vitest run src/components/__tests__/styles-grafito.test.ts` que los nuevos fallan por el radio

## 2. Green

- [ ] 2.1 En `src/styles.css`, poner `--control-radius: 0` y radio 0 en todos los selectores del 1.1 (salvo las excepciones circulares). Verificar que los tests pasan sin regresiones

## 3. Refactor y verificación

- [ ] 3.1 Agrupar los radios 0 en la regla de la sección Grafito y eliminar las declaraciones de radio que queden redundantes, manteniendo los tests en verde
- [ ] 3.2 Mutación manual sobre el diff (revertir cada regla y comprobar que su test falla, máximo 3 min) y anotar el resultado en `mutation-testing.md`
- [ ] 3.3 `npm test` y `npm run build` en verde, incluido el golden `series-tokens.json`

## Workflow follow-up

- Archivar con `openspec archive controles-rectos --yes` y comprobar que `openspec/specs/interfaz/spec.md` refleja los requisitos modificados y añadidos.
