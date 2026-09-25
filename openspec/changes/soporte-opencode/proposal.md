# Proposal

## Why

El usuario también trabaja con OpenCode y su uso no aparece: OpenCode no escribe JSONL sino una base SQLite (`~/.local/share/opencode/opencode.db`) con sesiones, mensajes y partes. Hace falta un provider que la lea de forma incremental.

## What Changes

- Provider "OpenCode" que abre `opencode.db` en solo lectura y convierte mensajes `assistant` en llamadas, mensajes `user` en turnos, partes `tool` en usos de herramienta (nombres normalizados a los de Claude Code: bash → Bash, edit → Edit…) y sesiones hijas en subagentes.
- Lectura incremental por cursor de `time_updated` guardado en `file_state`.
- Columna `cost_reported` en `calls` y vista `call_costs` que la usa cuando el modelo no tiene precio en la tabla (OpenCode calcula el coste por mensaje).
- El trait `Provider` distingue fuentes por líneas (JSONL) y por base SQLite.

## Capabilities

### New Capabilities
- `ingesta-opencode`: detección y lectura incremental de la base de OpenCode.

### Modified Capabilities
- `precios-modelos`: una llamada con coste reportado por el agente y sin precio en la tabla usa ese coste.

## Impact

- `src-tauri/src/providers/{mod,opencode}.rs`, `ingest.rs`, migración `0004`, fixture SQL en `tests/fixtures/opencode/`.
