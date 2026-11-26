# Proposal

## Why

La interfaz ya es recta (radio 0), pero la miniatura de vista previa del tema en Ajustes conserva un radio de 8 px en el lienzo y formas internas redondeadas (barras, chips y tarjetas), de modo que se ven esquinas redondas dentro de una caja recta. Hay que cuadrarla para que sea coherente con el resto.

## What Changes

- La miniatura de tema (claro, oscuro y la combinada de «Sistema») se dibuja con esquinas rectas en su lienzo.
- Las tarjetas, barras, entradas de menú y chips dibujados dentro de la miniatura pasan a ser rectángulos sin redondeo, reflejando el estilo actual.
- El logotipo de la miniatura sigue siendo un círculo. No cambian los colores ni los iconos.

## Capabilities

### New Capabilities

### Modified Capabilities
- `interfaz`: el requisito «Elementos de interfaz rectos» precisa que la miniatura de tema es recta también por dentro (lienzo y formas dibujadas).

## Impact

- `src/components/SettingsDialog.tsx` (miniatura de tema).
- Nuevo test de componente en `src/components/__tests__/`.
