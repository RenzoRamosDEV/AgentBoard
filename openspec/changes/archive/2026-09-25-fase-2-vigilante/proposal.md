# Proposal

## Why

Ahora el historial solo se lee al arrancar: si dejas AgentBoard abierto y sigues usando los agentes, las cifras se quedan congeladas hasta reiniciar. El doc original pedía una "vista en vivo" con el evento en la UI en menos de 1 s. Esta es la fase 2 del roadmap y el hueco más importante que queda.

## What Changes

- Un vigilante de archivos (crate `notify`) observa las carpetas de logs de cada agente detectado.
- Al cambiar un archivo, agrupa los eventos 400 ms (debounce) y relee de forma incremental (los offsets de `file_state` evitan releer todo), y luego avisa a la UI con `ingest://done`.
- La UI ya escucha ese evento y refresca; no cambia el frontend salvo un pequeño indicador opcional de "en vivo".
- Si aparece la carpeta de un agente que aún no existía, el vigilante empieza a observarla sin reiniciar.

## Capabilities

### New Capabilities
- `vigilancia-en-vivo`: relectura automática de los logs mientras la app está abierta.

## Impact

- `src-tauri/src/watcher.rs` (nuevo), `lib.rs`, `Cargo.toml` (dependencia `notify`).
