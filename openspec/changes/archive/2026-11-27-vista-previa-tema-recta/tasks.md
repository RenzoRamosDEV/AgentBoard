# Tasks

## 1. Red

- [x] 1.1 Añadir test de componente que renderice la pestaña Tema de Ajustes y compruebe que ningún lienzo de miniatura tiene `border-radius` distinto de 0 y que ninguna forma SVG tiene `rx`/`ry` distinto de 0 (el logotipo sigue siendo `circle`)
- [x] 1.2 Ejecutar el test y confirmar que falla por el radio actual; commit `test(ajustes): ...`

## 2. Green

- [x] 2.1 Quitar el `borderRadius` en línea y los `rx` de la miniatura en `SettingsDialog.tsx` sin cambiar colores ni geometría
- [x] 2.2 Ejecutar el test y confirmar que pasa; commit `feat(ajustes): ...`

## 3. Refactor y verificación

- [x] 3.1 Refactor si procede, con los tests en verde
- [x] 3.2 Mutación sobre el diff (Stryker si está configurado; si no, manual) y anotar el resultado
- [x] 3.3 `npm test` y `npm run build` en verde

## Notas de verificación

- Refactor: no procede; el cambio solo elimina el `borderRadius` en línea y los `rx` de la miniatura.
- Mutación: Stryker no está configurado en el proyecto, así que se hizo mutación manual sobre el diff revirtiendo cada línea cambiada por separado (el radio del lienzo y los 8 `rx` de barras, chips y tarjetas): 9/9 mutantes muertos, ninguno superviviente.
- `npm test`: 25 ficheros, 274 tests en verde. `npm run build`: correcto.
