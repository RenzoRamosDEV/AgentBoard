# Mutation testing

El diff solo toca `src/styles.css` y tests; no hay herramienta de mutación para CSS, así que se aplicaron mutantes a mano sobre las reglas del diff, uno a uno, ejecutando `npx vitest run src/components/__tests__/styles-grafito.test.ts` y restaurando el CSS después de cada uno.

**Resultado: 15/15 mutantes muertos (100 %) en 15 s.**

| Grupo | Mutantes |
|---|---|
| Separación degradada | quitar el `border-image` de `.sidebar`; ángulo `180deg` → `135deg`; arranque `--card-edge` → `--accent`; tramo 40 % → 50 %; tramo 65 % → 70 %; final `--border` → `transparent`; `border-image-slice` 1 → 0 |
| Barra del activo | quitar el `box-shadow`; 2 px → 3 px; sin `inset`; color `--accent`; barra a la derecha (`-2px`); añadir `padding-left` al activo |
| Modos plegado y estrecho | la media query de 940 px redefine `border-right`; `.sidebar.collapsed` anula el `border-image` |

En la primera pasada el mutante del ángulo sobrevivió porque se aplicó por error al primer `180deg` del archivo (la rotación de la flecha del desplegable, fuera del diff); al apuntarlo a la regla del panel murió.
