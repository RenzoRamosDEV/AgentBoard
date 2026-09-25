# Servidor MCP

Además de la interfaz, AgentBoard incluye `agentboard-mcp`: un servidor que expone los mismos
datos de uso a cualquier agente de IA a través del **Model Context Protocol (MCP)**. Así un
agente puede preguntar, durante una conversación, cosas como «¿cuánto se lleva gastado este mes?»
o «¿qué modelo sale más caro?».

## Cómo funciona

`agentboard-mcp` es un binario aparte que reutiliza la misma lógica que la app (providers,
ingesta y consultas). Al arrancar escanea los logs del disco a una base en memoria —no necesita
que la aplicación esté abierta ni escribe nada en disco— y habla MCP por **stdio** (JSON-RPC 2.0):
responde a `initialize`, `tools/list` y `tools/call`.

Es un servidor MCP estándar, así que **cualquier cliente compatible con MCP** puede usarlo
(Claude Code, Codex, Gemini, Cursor, OpenCode…), registrándolo en cada uno con su propio método.

## Registro

```bash
# Compilar el binario
cd src-tauri && cargo build --release --bin agentboard-mcp

# Claude Code
claude mcp add agentboard -- /ruta/a/agentboard-mcp

# Codex CLI (~/.codex/config.toml)
[mcp_servers.agentboard]
command = "/ruta/a/agentboard-mcp"

# Gemini CLI (~/.gemini/settings.json)
{ "mcpServers": { "agentboard": { "command": "/ruta/a/agentboard-mcp" } } }
```

## Herramientas

Todas aceptan un filtro opcional: `period` (`7d`, `30d`, `60d`, `90d`, `all`), `agents` (ids) y
`projects` (ids).

| Herramienta | Devuelve |
| --- | --- |
| `get_summary` | Coste, llamadas, sesiones, cache hit, ahorro por caché, burn rate y modelos sin precio. |
| `get_cost_by_agent` / `_model` / `_project` / `_branch` | Desglose de coste y uso. |
| `get_activity` | Reparto por tipo de actividad. |
| `get_tools` / `get_shell_commands` | Uso y errores por herramienta o comando. |
| `get_skills` / `get_mcp_servers` / `get_agent_types` | Skills y subagentes, servidores MCP y tipos de subagente. |
| `get_daily` | Serie diaria (coste, llamadas, sesiones, tokens). |
| `list_agents` / `list_projects` | Detectados en la máquina, con su coste. |
| `get_data_info` | Rango de fechas y totales del historial. |

Si una herramienta recibe argumentos inválidos, el servidor devuelve un error legible sin cerrar
la conexión.
