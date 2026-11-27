# Proposal

## Why

`openspec validate --specs --strict` falla en `interfaz` y `sesiones` porque «Hoy en directo» (509 caracteres) y «Listado de sesiones» (516) superan el límite de 500. Hay que acortarlos sin cambiar lo que exigen.

## What Changes

- `interfaz` / «Hoy en directo»: la ventana de 15 minutos que define una sesión activa sale del texto y pasa al nuevo escenario «Ventana de actividad de 15 minutos». Se conservan todos los escenarios.
- `sesiones` / «Listado de sesiones»: se compacta la redacción de la paginación y el orden; el tope de 500 y el orden por defecto quedan además cubiertos por los nuevos escenarios «Tope de página» y «Orden por defecto». Se conservan todos los escenarios.
- No se parten los requisitos: OpenSpec no permite que un MODIFIED quite escenarios, así que partirlos obligaría a duplicar escenarios o a renombrar requisitos.
- Sin cambios de comportamiento ni de código.

## Capabilities

### New Capabilities

### Modified Capabilities
- `interfaz`: «Hoy en directo» se acorta y gana un escenario.
- `sesiones`: «Listado de sesiones» se acorta y gana dos escenarios.

## Impact

- Solo `openspec/specs/interfaz/spec.md` y `openspec/specs/sesiones/spec.md`. Ningún código ni test.
