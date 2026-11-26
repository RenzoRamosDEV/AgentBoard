# Design

## Context

`.sidebar` (src/styles.css) ya es `position: relative`, tiene `border-right: 1px solid var(--border)` y `overflow-y: auto`. Las tarjetas dibujan su borde degradado con un `::before` absoluto y máscara (ver «Tarjetas rectas con borde degradado»). `.nav-item.active` usa fondo `--surface-3`, texto primario e icono en `--accent`.

## Goals / Non-Goals

**Goals:** separación degradada vertical y barra de acento en el activo, válidas en panel desplegado, plegado y en la media query de 940 px.

**Non-Goals:** cambiar colores, tamaños, padding o componentes React.

## Decisions

- **`border-image` en vez de pseudo-elemento para la separación.** El panel tiene `overflow-y: auto`: un `::after` absoluto con `top: 0; bottom: 0` mide la altura visible pero se desplaza con el contenido al hacer scroll, dejando la línea cortada. `border-image: linear-gradient(180deg, var(--card-edge), transparent 40%, transparent 65%, var(--border)) 1` pinta el degradado sobre el propio `border-right` de 1 px, que no se desplaza, no captura el ratón y no duplica el borde sólido (lo sustituye). Como solo el borde derecho tiene anchura, solo se pinta ese lado. Los modos plegado y estrecho solo cambian la anchura, así que lo heredan sin reglas extra. Se conserva `border-right: 1px solid var(--border)` como anchura y como respaldo si `border-image` no se soporta.
- **Barra del activo con `box-shadow: inset 2px 0 0 var(--card-edge)`.** No ocupa espacio en la caja, así que texto e iconos no se mueven (ni con `padding: 0` del modo plegado) y cubre toda la altura del elemento con esquinas rectas.
- **Se conserva el fondo gris del activo.** El hover ya usa `--surface-2`; sin fondo, el activo se vería menos marcado que un elemento con el ratón encima, y en el modo plegado (solo iconos de 60 px) una barra de 2 px sola es una señal débil. Barra + fondo mantiene la jerarquía y añade el acento de las tarjetas.

## Risks / Trade-offs

- `border-image` ignora `border-radius`, irrelevante porque todo es recto.
