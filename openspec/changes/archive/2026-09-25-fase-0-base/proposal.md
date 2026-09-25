# Proposal

## Why

AgentBurn necesita una base sobre la que construir todas las vistas: una app Tauri que arranque, una base SQLite local con esquema versionado y un parser de Claude Code que importe el historial real. Sin esto no hay datos que enseñar ni forma de comprobar que los totales cuadran.

## What Changes

- Proyecto Tauri 2 con núcleo Rust (`src-tauri/`) e interfaz React + TypeScript + Vite (`src/`).
- Base SQLite en modo WAL en la carpeta de datos estándar del sistema, con migraciones versionadas en `src-tauri/migrations/` aplicadas al arrancar.
- Esquema de ocho tablas (`agents`, `projects`, `sessions`, `calls`, `tool_calls`, `events`, `prices`, `file_state`) y la vista `call_costs`.
- Trait `Provider` y primera implementación para Claude Code (`~/.claude/projects/`, respetando `CLAUDE_CONFIG_DIR`).
- Escaneo inicial que importa todo el historial en disco, con offsets por archivo y upsert por `message_id` para no duplicar.
- Tabla de precios inicial embebida (USD por millón de tokens) y coste calculado en consulta.
- Comando `get_summary` y una pantalla mínima que muestra coste, llamadas y sesiones totales.
- Tests del parser con fixtures JSONL anonimizados.

## Capabilities

### New Capabilities
- `almacenamiento-local`: dónde vive la base, cómo se versiona el esquema y cómo se guardan fechas.
- `ingesta-claude-code`: detección de los logs de Claude Code y conversión de cada línea en filas sin duplicados.
- `precios-modelos`: cálculo del coste de cada llamada a partir de los tokens y la tabla de precios vigente.

### Modified Capabilities

## Impact

- Código nuevo: todo el repositorio (`src/`, `src-tauri/`, `tests/fixtures/`).
- Dependencias: Tauri 2, rusqlite (bundled, FTS5), serde, dirs, notify (se usa en la fase 2), React, Vite.
- Sistema: en Linux requiere WebKitGTK 4.1 para compilar; en Bazzite se compila dentro de un distrobox.
