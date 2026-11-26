# Tasks

## 1. Red

- [ ] 1.1 Añadir test de componente que renderice la pestaña Tema de Ajustes y compruebe que ningún lienzo de miniatura tiene `border-radius` distinto de 0 y que ninguna forma SVG tiene `rx`/`ry` distinto de 0 (el logotipo sigue siendo `circle`)
- [ ] 1.2 Ejecutar el test y confirmar que falla por el radio actual; commit `test(ajustes): ...`

## 2. Green

- [ ] 2.1 Quitar el `borderRadius` en línea y los `rx` de la miniatura en `SettingsDialog.tsx` sin cambiar colores ni geometría
- [ ] 2.2 Ejecutar el test y confirmar que pasa; commit `feat(ajustes): ...`

## 3. Refactor y verificación

- [ ] 3.1 Refactor si procede, con los tests en verde
- [ ] 3.2 Mutación sobre el diff (Stryker si está configurado; si no, manual) y anotar el resultado
- [ ] 3.3 `npm test` y `npm run build` en verde
