# Proposal

## Why

Tras el rediseño Grafito las tarjetas llevan un borde de 1 px degradado que arranca en el color de acento de tarjeta (`--card-edge`), pero el panel lateral sigue separado del contenido por una línea gris uniforme y su elemento activo solo se distingue por un fondo gris, así que no comparte el lenguaje visual de las tarjetas.

## What Changes

- La línea vertical que separa el panel lateral del contenido pasa a ser un degradado vertical: `--card-edge` arriba, transparente entre el 40 % y el 65 % y `--border` abajo, como el borde de las tarjetas. Se mantiene con el panel desplegado, plegado (solo iconos) y en ventanas estrechas.
- El elemento activo del panel lateral lleva a la izquierda una barra vertical recta de 2 px en `--card-edge`, a toda su altura, sin desplazar texto ni iconos; conserva el fondo gris y el texto en color primario.
- Sin cambios de colores de series (golden intacto), tamaños ni componentes React.

## Capabilities

### New Capabilities

_Ninguna._

### Modified Capabilities

- `interfaz`: se añaden «Separación degradada del panel lateral» y «Acento del elemento activo del panel lateral». Los requisitos existentes «Panel lateral agrupado» y «Detalles del panel lateral» no describen el borde ni el estilo del activo, así que no se modifican.

## Impact

- `src/styles.css`: sección Grafito (`.sidebar`, `.nav-item.active`).
- `src/components/__tests__/styles-grafito.test.ts`: nuevos tests de estilo.
- Sin cambios en componentes, backend, API ni dependencias.
