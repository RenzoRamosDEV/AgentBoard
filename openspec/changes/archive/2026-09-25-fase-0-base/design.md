# Design

## Context

Repositorio vacío; la especificación de producto (Claude Doc "AgentBoard — especificación y arquitectura") fija Tauri 2 + Rust + SQLite + React. El entorno de desarrollo es Bazzite (inmutable): la compilación nativa se hace dentro de un distrobox Fedora con WebKitGTK 4.1; Node vive en `~/.nvm` y Rust en `~/.cargo`, ambos compartidos con el contenedor.

## Goals / Non-Goals

**Goals:**
- Núcleo Rust separado de Tauri en lo posible (`db`, `ingest`, `providers`, `pricing`) para testearlo con `cargo test` sin abrir ventana.
- Importar el historial real de Claude Code de esta máquina y mostrar totales.

**Non-Goals:**
- Watcher en vivo, dashboard completo, Codex/Gemini, bandeja y CI (fases 1–5).
- Descarga de precios de LiteLLM (fase posterior; se deja el punto de extensión).

## Decisions

- **rusqlite con `bundled`** en vez de sqlx: API síncrona sencilla, SQLite con FTS5 compilado dentro y sin depender de la versión del sistema. La ingesta corre en un hilo propio; la UI consulta con una conexión aparte (WAL).
- **Migraciones propias** con `PRAGMA user_version` y archivos `NNNN_nombre.sql` incluidos con `include_str!`: cero dependencias y orden explícito. Alternativa descartada: `refinery` (más peso para 1–2 migraciones).
- **`calls.message_id` como PRIMARY KEY** y `INSERT … ON CONFLICT DO UPDATE`: el streaming de Claude Code repite el id; la última línea trae los tokens finales.
- **Coste en una vista SQL** (`call_costs`) con `LEFT JOIN` a `prices`: modelos sin precio siguen contando con coste 0. Los precios se guardan en USD por millón.
- **Identificador de archivo**: inode + dispositivo en Unix, `file_index` en Windows (vía `std::os::windows::fs::MetadataExt` cuando esté estable; por ahora tamaño + mtime como respaldo en Windows).
- **Tool results**: el `tool_result` llega en una línea `user` posterior; se hace `UPDATE tool_calls SET is_error=1 WHERE call_id=?`, así el orden de llegada no importa si ambas líneas están en el mismo lote o en lotes distintos.
- **Proyecto**: `cwd` completo como clave; nombre = última carpeta; si el `cwd` contiene `/.claude/worktrees/`, se agrupa bajo la ruta anterior.
- **Estructura del crate**: `src-tauri/src/lib.rs` expone los módulos; `main.rs` solo arranca Tauri. Los tests de integración usan `tests/fixtures/claude_code/*.jsonl`.

## Risks / Trade-offs

- [El formato JSONL de Claude Code cambia entre versiones] → parser tolerante: campos opcionales, líneas desconocidas ignoradas, fixtures basados en la versión 2.1.x instalada.
- [Historiales grandes tardan en el primer escaneo] → transacción por archivo y lectura en buffer; la UI muestra datos al terminar.
- [Precios embebidos se quedan viejos] → tabla `prices` con `valid_from`; la fase de precios dinámicos la actualizará sin reimportar.
