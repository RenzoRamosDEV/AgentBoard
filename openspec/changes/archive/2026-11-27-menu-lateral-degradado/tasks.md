# Tasks

## 1. Red

- [x] 1.1 En `src/components/__tests__/styles-grafito.test.ts`, añadir tests: `.sidebar` declara `border-image: linear-gradient(180deg, var(--card-edge), transparent 40%, transparent 65%, var(--border)) 1` sobre `border-right` de 1 px; la media query de 940 px no redefine `border`, `border-right` ni `border-image` de `.sidebar`; `.nav-item.active` declara `box-shadow: inset 2px 0 0 var(--card-edge)`, conserva `background: var(--surface-3)` y `color: var(--text-primary)` y no cambia `padding`. Verificar con `npx vitest run src/components/__tests__/styles-grafito.test.ts` que los nuevos fallan por la ausencia de esas declaraciones

## 2. Green

- [x] 2.1 En la sección Grafito de `src/styles.css`, añadir el `border-image` a `.sidebar` y el `box-shadow` inset a `.nav-item.active`. Verificar que los tests pasan sin regresiones

## 3. Refactor y verificación

- [x] 3.1 Refactor si procede, manteniendo los tests en verde
- [x] 3.2 Mutación manual sobre el diff (revertir cada regla y comprobar que su test falla, máximo 3 min) y anotar el resultado en `mutation-testing.md`
- [x] 3.3 `npm test` y `npm run build` en verde, incluido el golden `series-tokens.json`
- [x] 3.4 Verificación visual con Vite en el puerto 5199 y capturas en claro y oscuro (desplegado; comprobar también el activo)

## Workflow follow-up

- Archivar con `openspec archive menu-lateral-degradado --yes` y comprobar que `openspec/specs/interfaz/spec.md` refleja los requisitos añadidos.
