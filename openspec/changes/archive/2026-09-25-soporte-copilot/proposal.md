# Proposal

## Why

El usuario también quiere ver el uso de GitHub Copilot CLI. Sus sesiones viven en `~/.copilot/session-state/<id>/events.jsonl`, un flujo de eventos con un esquema propio en el que los tokens de entrada solo aparecen agregados al cerrar la sesión.

## What Changes

- Provider "GitHub Copilot CLI" que lee `events.jsonl` de `COPILOT_HOME` (por defecto `~/.copilot`) con estado por archivo.
- Una llamada por `assistant.message` (tokens de salida y modelo); en `session.shutdown` el delta por modelo de entrada, caché y razonamiento se reparte entre las respuestas de ese modelo desde el cierre anterior.
- Turnos desde `assistant.turn_start` con la intención del `user.message` previo; herramientas desde `tool.execution_start/complete` (nombres normalizados), subagentes y skills desde `subagent.started` y `skill.invoked`.
- Normalización de modelos con punto (`claude-opus-4.7` → `claude-opus-4-7`) y sufijo `-1m`.

## Capabilities

### New Capabilities
- `ingesta-copilot`: detección y conversión de las sesiones de GitHub Copilot CLI.

### Modified Capabilities

## Impact

- `src-tauri/src/providers/copilot.rs`, `pricing.rs` (normalización y precios), fixture en `tests/fixtures/copilot/`.
