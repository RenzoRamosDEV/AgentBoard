# Proposal

## Why

Codex CLI es el segundo agente del roadmap y el usuario quiere ver su uso junto al de Claude Code y OpenCode. Sus sesiones son JSONL en `~/.codex/sessions/` con un formato propio (líneas `session_meta`, `turn_context`, `token_usage_record`, `response_item`, `event_msg`).

## What Changes

- Provider "Codex CLI" que lee `rollout-*.jsonl` de `sessions/` y `archived_sessions/` (respetando `CODEX_HOME`), con estado por archivo para el modelo, la carpeta, la rama y si es subagente.
- Llamadas desde `token_usage_record` (una por respuesta, con `response_id`); los `token_count` antiguos solo cuando no hay registros.
- Turnos desde `turn_context` + `user_message`; herramientas desde `function_call`, `custom_tool_call` y `local_shell_call` con nombres normalizados (shell → Bash, apply_patch → Edit…) y MCP como `mcp__<ns>__<tool>`.
- Precios de los modelos de Codex (gpt-5.1-codex, codex-mini…).

## Capabilities

### New Capabilities
- `ingesta-codex`: detección y conversión de las sesiones de Codex CLI.

### Modified Capabilities

## Impact

- `src-tauri/src/providers/codex.rs`, `pricing.rs`, fixtures en `tests/fixtures/codex/`.
