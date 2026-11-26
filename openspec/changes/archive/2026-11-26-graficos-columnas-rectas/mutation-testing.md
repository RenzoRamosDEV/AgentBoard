# Mutation testing

El diff solo toca `src/styles.css` y tests; no hay herramienta de mutación para CSS, así que se aplicaron mutantes a mano sobre las reglas del diff, uno a uno, ejecutando `npx vitest run src/components/__tests__/styles-grafito.test.ts` y restaurando el CSS después de cada uno.

**Resultado: 13/13 mutantes muertos (100 %) en 18 s.**

| Grupo | Mutantes |
|---|---|
| Regla agrupada de radio 0 | quitar del grupo `.column-seg`, `.column-single`, `.with-dot i`, `.legend i`, `.share-list i`, `.agent-dots i` y `.palette-dot` (7); poner el grupo a `2px` |
| Columnas apiladas | reintroducir los radios de `.column-stack > .column-seg:first-child`, `:last-child` y `:only-child` |
| Muestras | `.palette-dot` a `7px` de ancho |
| Círculos intactos | `.status-dot` con radio 0 |

Reintroducir `border-radius: 2px` en las reglas originales de las muestras, o `5px` en la regla original de `.column-single`, son mutantes equivalentes: la regla agrupada de Grafito va después y los sobrescribe, así que el estilo calculado no cambia.
