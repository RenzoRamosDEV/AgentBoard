# Proposal

## Why

Tras el rediseño Grafito las tarjetas, chips, segmentados, barras, columnas y leyendas son rectas, pero la barra superior y el resto de controles siguen redondeados (desplegables «Agentes/Proyectos», exportar, buscador y su `kbd`, insignia de versión, interruptor «Comparar con periodo anterior», píldora «En vivo») y desentonan con lo recto.

## What Changes

- El radio común de los controles pasa de 8 px a 0: botones, botones de filtro, desplegables, campos de texto, buscadores y campos de presupuesto quedan rectos.
- Radio 0 también en etiquetas e insignias (`kbd`, versión, «nuevo», `badge`, `tag`, origen de precio, píldora de comparación, «En vivo»), elementos de lista y navegación (entradas del panel lateral, botón de plegar, opciones de menú y desplegable, resultados del buscador rápido, filas de lista de casillas, filas de sesión de hoy), campos de precio y de rango, miniatura de tema y anillo de foco.
- Interruptor: pista rectangular y pulgar cuadrado.
- Ventanas flotantes (popover, diálogo, buscador rápido, lista de desplegable y tooltip) rectas, conservando su sombra.
- Se mantienen circulares el logotipo y los puntos de estado y en vivo.
- Sin cambios de colores (golden de series intacto), alturas ni tamaños de letra de los controles.

## Capabilities

### New Capabilities

_Ninguna._

### Modified Capabilities

- `interfaz`: se modifican «Consistencia de tamaños», «Elementos rectos dentro de las tarjetas» y «Columnas y leyendas de gráficos rectas» (el interruptor deja de ser excepción circular); se añaden «Elementos de interfaz rectos» y «Ventanas flotantes rectas».

## Impact

- `src/styles.css`: token `--control-radius` y radios de los selectores citados; se agrupan en la regla de radio 0 de la sección Grafito.
- `src/components/__tests__/styles-grafito.test.ts`: los tests que fijaban 8 px pasan a exigir 0 y se añaden los nuevos.
- Fuera de alcance: la ilustración SVG de la miniatura de tema en Ajustes (dibujo, no control) y los iconos.
- Sin cambios en componentes React, backend, API ni dependencias.
