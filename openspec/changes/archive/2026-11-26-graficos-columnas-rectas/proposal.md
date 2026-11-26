# Proposal

## Why

Tras el rediseño Grafito, las tarjetas, chips y barras horizontales son rectas, pero las columnas de los gráficos y las muestras de color de las leyendas siguen con esquinas redondeadas (5 px y 2 px) y desentonan con el resto.

## What Changes

- Las columnas de los gráficos de columnas (apiladas y simples) pasan a tener esquinas rectas: se eliminan los radios de 5 px de sus segmentos primero, último y único.
- Las muestras de color de 8 px de leyendas, listas de reparto, puntos de agente y buscador rápido pasan a radio 0.
- Sin cambios en los puntos circulares (estado, en vivo, logotipo, interruptor), en las líneas de los gráficos (extremos redondeados) ni en los colores de series.
- No hay rectángulos SVG de barras con `rx`/`ry` en los gráficos: el único `<rect>` de `Charts.tsx` es una capa transparente para el ratón, y los de la miniatura de tema de Ajustes no son gráficos; no se tocan.

## Capabilities

### New Capabilities

_Ninguna._

### Modified Capabilities

- `interfaz`: se añade el requisito «Columnas y leyendas de gráficos rectas».

## Impact

- `src/styles.css`: reglas de `.column-seg`, `.column-single`, `.column-stack > .column-seg:*-child`, `.with-dot i`, `.legend i`, `.share-list i`, `.agent-dots i`, `.palette-dot` y la regla agrupada de radio 0 de la sección Grafito.
- `src/components/__tests__/styles-grafito.test.ts`: tests nuevos.
- Sin cambios en componentes React, backend, API ni dependencias.
