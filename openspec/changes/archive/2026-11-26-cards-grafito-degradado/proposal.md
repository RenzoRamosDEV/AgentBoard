# Proposal

## Why

La interfaz actual usa grises azulados, tarjetas con relleno, radio de 12 px y bordes de 1 px que la hacen pesada. Se busca un aspecto más simple y moderno: grises y negros neutros, líneas finas y tarjetas de esquinas rectas. Tras comparar cuatro temas y cinco variantes de línea en la maqueta `docs/design/propuesta-temas-cards.html`, se eligió el tema **Grafito** con la línea **«E · Degradado»**.

## What Changes

- Nueva paleta neutra «Grafito» para fondos, superficies, bordes, separadores, texto y pista de barras, en modo oscuro y en modo claro.
  - Oscuro: fondo `#0f0f10`; superficies `#161618` / `#1c1c1f` / `#232326`; borde `#262629`; separador de filas `#1e1e21`; texto primario `#ededee`, secundario `#b4b4b8`, atenuado `#77777d` (se ajusta si no cumple contraste AA 4.5:1 sobre el fondo); pista `#232326`.
  - Claro: fondo `#f4f4f5`; superficies `#ffffff` / `#f0f0f2` / `#ececef`; borde `#e4e4e7`; separador de filas `#f0f0f2`; texto primario `#111113`, secundario `#4a4a50`, atenuado `#8a8a92` (mismo criterio de contraste); pista `#ececef`.
- Tarjetas (`.panel`, `.kpi` y cualquier contenedor tipo tarjeta): radio 0, sin sombra, sin relleno propio y con borde de 1 px degradado a 135° (acento arriba a la izquierda → transparente entre el 40 % y el 65 % → gris de borde abajo a la derecha).
- Nuevo token `--card-edge` con el color de arranque del degradado: naranja `#f2a33a` en oscuro y celeste `#20b0f0` en claro (los mismos valores que `--accent-soft` en cada modo).
- Líneas internas (separadores de tablas, `panel-note`, cabeceras de tabla) de 0,5 px.
- Radio 0 en chips, pestañas y controles segmentados, barras de medidor y `card-alert`. Los botones, desplegables y campos conservan su radio.
- Sin cambios en los colores de gráficos y series (`--palette-*`, `--series-*`, `--act-*`, `--agent-*`), la estructura de los paneles, el contenido, el comportamiento ni la tipografía.
- El selector de tema (oscuro/claro/sistema) sigue funcionando y ambos modos reciben el rediseño.

## Capabilities

### New Capabilities

_Ninguna._

### Modified Capabilities

- `interfaz`: se añaden requisitos de estilo visual (paleta Grafito con contraste AA, tarjetas rectas con borde degradado y token `--card-edge`, líneas internas finas, radio 0 en chips/segmentados/medidores/avisos, colores de series intactos) y se ajusta «Consistencia de tamaños» para que los controles segmentados dejen de compartir el radio de los controles de formulario.

## Impact

- `src/styles.css`: tokens de `:root` y `:root[data-theme="light"]`, reglas de `.panel`, `.kpi`, `.notice`, `.theme-card`, separadores de tabla, `.panel-note`, `.chip`, `.segmented`/`.segment`, `.bar-track`/`.bar-fill`, `.inline-bar`, `.share-bar`, `.card-alert`.
- Nuevo test de estilos en `src/components/__tests__/` que lee `src/styles.css`, al estilo de `charts.test.ts`.
- Sin cambios en componentes React, backend Rust, API ni dependencias.
- La maqueta `docs/design/propuesta-temas-cards.html` se versiona como referencia visual.
