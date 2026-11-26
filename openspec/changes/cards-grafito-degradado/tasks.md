# Tasks

Ciclo TDD en cada grupo: primero los tests en rojo, luego el CSS mínimo para ponerlos en verde, después refactor. Todos los tests nuevos van en `src/components/__tests__/styles-grafito.test.ts` (estilo de `charts.test.ts`: `readFileSync` de `../../styles.css`). Commits pequeños en español con Conventional Commits, sin `Co-Authored-By`.

## 1. Red de seguridad de colores de series (golden)

- [x] 1.1 Crear `styles-grafito.test.ts` con el helper `declarations(css, selector)` (bloques de selector exacto, espacios normalizados, la última declaración gana) y un test unitario del helper con un CSS mínimo en línea (selector ausente → mapa vacío; selector repetido → gana el último); verificar con `npx vitest run src/components/__tests__/styles-grafito.test.ts`
- [x] 1.2 Añadir el test golden «colores de series intactos»: extrae todos los `--palette-*`, `--series-*`, `--act-*`, `--agent-*` de `:root` y `:root[data-theme="light"]` y los compara con `__golden__/series-tokens.json`; generar el golden con `UPDATE_GOLDEN=1` **sobre el CSS actual, antes de tocarlo**, y verificar que el test pasa en verde y que el JSON tiene 30 entradas por modo (8 `--palette-*`, 6 `--series-*`, 11 `--act-*`, 5 `--agent-*`)

## 2. Paleta Grafito y contraste

- [x] 2.1 Red: tests por partición oscuro/claro con los valores exactos de `--bg`, `--sidebar`, `--surface-1..3`, `--border`, `--row-border`, `--text-primary`, `--text-secondary` y `--track` (design, decisión 1) y test de contraste WCAG `--text-muted` vs `--bg` ≥ 4.5 en ambos modos, más un caso límite que confirma que `#77777d` sobre `#0f0f10` da < 4.5; verificar que fallan
- [ ] 2.2 Verde: reasignar esos tokens en `:root` y `:root[data-theme="light"]` de `src/styles.css`, con `--text-muted` `#848489` (oscuro) y `#6b6b73` (claro); verificar que los tests de 2.1 y el golden de 1.2 pasan

## 3. Tarjetas rectas con borde degradado

- [ ] 3.1 Red: tests de `--card-edge` (`#f2a33a` en `:root`, `#20b0f0` en claro); de `.panel` y `.kpi` con `border-radius: 0`, `background: transparent`, `box-shadow: none`, `border: 0` y `position: relative`; y de `.panel::before` / `.kpi::before` con `pointer-events: none`, `padding: 1px`, `inset: 0`, un `linear-gradient(135deg, var(--card-edge), transparent 40%, transparent 65%, var(--border))`, `mask-composite: exclude` y `-webkit-mask-composite: xor`; verificar que fallan
- [ ] 3.2 Verde: añadir `--card-edge` a ambos bloques y las reglas de tarjeta y `::before` (design, decisión 4), compensando 1 px de `padding` en `.panel`, `.kpi` y `.kpis-compact .kpi`; verificar que los tests de 3.1 pasan
- [ ] 3.3 Red + verde: test de radio 0 en `.notice`, `.today-session` y `.theme-card` (design, decisión 5), y cambio del CSS; verificar que pasa

## 4. Líneas internas y elementos rectos

- [ ] 4.1 Red: tests de grosor `0.5px` en `.table-head` y `.table-row` (`border-bottom`) y en `.panel-note` (`border-top`); de `border-radius: 0` en `.chip`, `.segmented`, `.segment`, `.segment.active`, `.bar-track`, `.bar-fill`, `.inline-bar`, `.inline-bar-fill`, los segmentos de `.share-bar` y `.card-alert`; y de que `.button`, `.select`, `.chip-button` y `.search` siguen con `var(--control-radius)` y `--control-radius` sigue en `8px`; verificar que los nuevos fallan
- [ ] 4.2 Verde: aplicar los cambios en `src/styles.css` (design, decisiones 6 y 7), quitando también la sombra de `.segment.active`; verificar que los tests de 4.1 pasan

## 5. Refactor y verificación de integración

- [ ] 5.1 Refactor: agrupar las reglas nuevas bajo un comentario de sección «Grafito» en `src/styles.css`, eliminar declaraciones que queden sin efecto (rellenos y radios sobrescritos) y renombrar helpers del test si hace falta; verificar que `npm test` sigue en verde
- [ ] 5.2 Mutation testing: el diff solo toca CSS y tests y el proyecto no tiene herramienta de mutación para CSS; documentar la justificación en la descripción del PR y comprobar a mano que cada test nuevo falla al revertir su regla (p. ej. poner `border-radius: 12px` en `.panel` o `transparent 50%` en el degradado) en menos de 3 minutos en total
- [ ] 5.3 Verificar `npm test` y `npm run build` en verde
- [ ] 5.4 Verificación visual con `npm run tauri dev` en tema oscuro, claro y sistema: tarjetas rectas y transparentes, borde naranja (oscuro) o celeste (claro) arriba a la izquierda que se apaga hacia el centro y vuelve a gris abajo a la derecha, enlaces y filas junto al borde pulsables, gráficos con los mismos colores; comparar con `docs/design/propuesta-temas-cards.html` (Grafito, «E · Degradado»)

## Workflow follow-up

- Archivar el cambio con `/opsx:archive` tras la revisión, sincronizando la delta de `interfaz`.
