# Proposal

## Why

AgentBoard ya sabe leer y agregar el uso de todos los agentes (coste, tokens, sesiones, actividad, herramientas, modelos, proyectos…), pero esos datos solo se ven en la interfaz. Un agente de IA (Claude Code, Codex, Gemini, etc.) no puede preguntarle a AgentBoard "¿cuánto llevo gastado hoy?" o "¿qué modelo me sale más caro?". Exponer estos datos por MCP permite que cualquier agente los consulte durante una conversación, sin depender de la interfaz.

## What Changes

- Nuevo binario `agentboard-mcp` en el mismo workspace de Cargo, que reutiliza `agentboard_lib` (providers + ingest + queries) y escanea los logs del disco a una base SQLite en memoria al arrancar. No necesita que la app esté abierta.
- Habla el protocolo MCP por **stdio** (JSON-RPC 2.0): `initialize`, `tools/list`, `tools/call`. Cualquier agente puede lanzarlo como servidor MCP local.
- Expone como herramientas MCP todo lo que hoy muestra el dashboard, con un filtro común (periodo, agentes, proyectos): resumen y costes, desgloses por agente/modelo/proyecto/actividad/herramientas/comandos/skills/MCP/subagentes, serie diaria y listados.
- Cada herramienta devuelve JSON estructurado (los mismos números que la interfaz).

## Non-goals

- No hospedar el MCP dentro de la app Tauri ni abrir puertos HTTP; el servidor es un binario aparte por stdio.
- No añadir vigilancia en vivo al servidor MCP: escanea al arrancar (una conexión MCP es de vida corta).
- No escribir nada en disco (misma filosofía que la app: solo lectura).

## Capabilities

### New Capabilities
- `servidor-mcp`: expone los datos de uso de AgentBoard a cualquier agente vía Model Context Protocol (stdio).

## Impact

- `src-tauri/Cargo.toml` (nuevo `[[bin]]` `agentboard-mcp`), `src-tauri/src/bin/mcp.rs` (nuevo), un módulo de protocolo MCP y otro que traduce las herramientas a `queries`.
- Reutiliza sin cambios `providers`, `ingest`, `queries`, `pricing`, `db`.
- Documentación de cómo registrarlo en Claude Code / Codex / Gemini.
