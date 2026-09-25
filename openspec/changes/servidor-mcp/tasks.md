# Tasks

## 1. Esqueleto del binario y protocolo MCP

- [ ] 1.1 Añadir `[[bin]]` `agentboard-mcp` en `Cargo.toml` y `src/bin/mcp.rs`; verificar con `cargo build --bin agentboard-mcp`
- [ ] 1.2 Bucle stdio JSON-RPC 2.0 (`initialize`, `tools/list`, `tools/call`, método desconocido → error); test unitario del enrutado

## 2. Datos

- [ ] 2.1 Escanear los logs a SQLite en memoria al arrancar reutilizando `providers::all` + `ingest::scan_all`
- [ ] 2.2 Parsear el filtro común (periodo/agentes/proyectos) a `queries::Filter`; test del parseo del periodo

## 3. Herramientas

- [ ] 3.1 `get_summary`, `get_cost_by_agent`, `get_cost_by_model`, `get_cost_by_project` con `inputSchema` y filtro
- [ ] 3.2 `get_daily` (serie diaria), `get_activity`, `get_tools`, `get_shell_commands`, `get_skills`, `get_mcp_servers`, `get_subagents`
- [ ] 3.3 `list_agents`, `list_projects`
- [ ] 3.4 Errores de herramienta controlados (argumentos/consulta) con `isError`; test de periodo inválido

## 4. Cierre

- [ ] 4.1 `cargo fmt`, `clippy -D warnings` y `cargo test` en verde
- [ ] 4.2 Documentar el registro del servidor en Claude Code / Codex / Gemini (README)
