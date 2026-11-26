# Tasks

## 1. Red

- [ ] 1.1 Añadir en `src/components/__tests__/styles-grafito.test.ts` tests de radio 0 para `.column-seg`, `.column-single`, `.with-dot i`, `.legend i`, `.share-list i`, `.agent-dots i` y `.palette-dot`; de que `.column-stack > .column-seg:first-child/last-child/only-child` no vuelven a redondear; y de que `.status-dot` y `.live-dot` siguen en `50%`. Verificar con `npx vitest run src/components/__tests__/styles-grafito.test.ts` que los nuevos fallan por el radio

## 2. Green

- [ ] 2.1 En `src/styles.css`, eliminar los radios de 5 px de las columnas y añadir `.column-seg` y `.column-single` a la regla agrupada de radio 0 de Grafito; poner a 0 el radio de las muestras de leyenda. Verificar que los tests del 1.1 pasan

## 3. Refactor y verificación

- [ ] 3.1 Quitar reglas redundantes que queden sin efecto, manteniendo los tests en verde
- [ ] 3.2 Mutación manual sobre el diff (revertir cada regla y comprobar que su test falla, máximo 3 min) y anotar el resultado en `mutation-testing.md`
- [ ] 3.3 `npm test` y `npm run build` en verde, incluido el golden `series-tokens.json`

## Workflow follow-up

- Archivar con `openspec archive graficos-columnas-rectas --yes` y comprobar que `openspec/specs/interfaz/spec.md` incluye el requisito nuevo.
