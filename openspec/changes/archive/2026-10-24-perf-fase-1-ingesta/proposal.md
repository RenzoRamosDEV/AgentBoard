# Proposal

## Why

La línea base de rendimiento (`docs/perf/BASELINE.md`, Fase 0) muestra que la ingesta incumple el presupuesto del proyecto: el escaneo frío de 200 MB tarda 15,5–16,9 s (presupuesto < 10 s) porque `Writer::apply` es el 83 % del escaneo y `sqlite3Prepare` el 57 % (cada registro recompila su SQL), y el refresco en vivo crece con el tamaño de la sesión (mediana de 200 refrescos 515 ms, de 170 a 1 046 ms) porque `backfill_turns` (58 % del perfil) recorre todas las llamadas sin turno de cada sesión tocada. Además, la ingesta y los comandos de la interfaz compiten por la misma conexión durante todo el escaneo, y con `panic = "abort"` cualquier `expect`/`unwrap` alcanzable desde los hilos de fondo cierra la app. Esta Fase 1 ataca los cuellos de botella #1 y #3 del ranking y deja la ingesta fuera de la interfaz, sin cambiar ningún dato importado.

## What Changes

- **Escritor con sentencias preparadas reutilizadas**: el escritor de SQLite prepara cada sentencia una vez por transacción/lote (`prepare_cached`) en lugar de recompilarla por registro (#1 de la línea base).
- **Relleno de turnos acotado**: el relleno de `turn_id` solo actúa cuando la pasada actual trae turnos nuevos y solo sobre las filas que esos turnos pueden afectar; un refresco sin turnos nuevos no recorre la sesión (#3 de la línea base).
- **Hilo de ingesta dedicado**: toda la ingesta (escaneo inicial y relecturas del vigilante) se ejecuta en un único hilo que recibe trabajos por `std::sync::mpsc`; el vigilante deja de escanear por sí mismo y solo encola trabajos.
- **Escaneo en paralelo por archivo**: lectura y parseo de archivos en paralelo (`std::thread::scope`, limitado al número de cores) y escritura secuencial en el orden actual (proveedor → raíz → ruta ordenada), por lotes que llegan al escritor por canal. El bloqueo de la base se toma solo para escribir cada lote, no durante la lectura ni el parseo.
- **Lectura incremental robusta**: estado por archivo `(ruta, identidad, offset, tamaño, mtime)` cargado una vez por escaneo; lectura por líneas con `BufReader::read_until` desde el offset; truncado o rotación reinician ese archivo; una última línea sin `\n` espera a la siguiente pasada; los archivos sin cambios se saltan sin abrirlos.
- **Parseo mínimo con sonda de tipo**: cada proveedor JSONL deserializa primero una sonda con solo los campos de envoltura (tipo, sesión, fecha) y descarta pronto las líneas que no producen filas; las líneas útiles se deserializan a structs con solo los campos necesarios, prestados (`&str`/`Cow`) cuando se pueda; el texto de los mensajes nunca se copia.
- **Comandos Tauri `async`**: los comandos no bloquean el hilo principal ni hacen I/O de disco síncrono en él; el trabajo con la base y con el archivo de ajustes va a `tauri::async_runtime::spawn_blocking`. La API de comandos (nombres, argumentos, respuestas) no cambia.
- **Sin pánicos en hilos de fondo**: se eliminan los `expect`/`unwrap` alcanzables desde los hilos de escaneo y vigilante (`alerts.rs:91`, `tz.rs:37,42,49,54`) y no se añade ninguno en la ingesta; un archivo corrupto se cuenta como error y no detiene el resto.
- **Herramientas de medición**: `perf_scan` añade la huella de resultados (para comparar antes/después) y las medianas del primer y último tramo de refrescos (para comprobar que el refresco no crece con la sesión).
- **Resultados documentados**: sección «Después de la Fase 1» en `docs/perf/BASELINE.md` con las cifras antes/después; un paso que no mejore se revierte y se explica.
- Sin cambios en los datos importados (mismas llamadas, tokens y costes), el esquema, la API de comandos, el protocolo MCP ni el frontend.

## Capabilities

### New Capabilities
<!-- Ninguna. -->

### Modified Capabilities
- `rendimiento`: añade requisitos de ingesta fuera de la interfaz, escaneo paralelo determinista, escritura sin recompilar sentencias, relleno de turnos acotado a la pasada, parseo mínimo sin materializar texto, aislamiento de fallos sin pánicos, equivalencia de resultados antes/después, presupuestos medibles de ingesta y comparación documentada con la línea base.

## Impact

- **Código**: `src-tauri/src/ingest.rs` (escritor, lectura incremental, pipeline paralelo), módulo nuevo del hilo de ingesta, `lib.rs` (escaneo inicial vía hilo de ingesta), `watcher.rs` (encola trabajos en vez de escanear), `commands.rs` (comandos `async` con `spawn_blocking`), `providers/{claude_code,codex,copilot,cursor,gemini}.rs` (sonda de tipo y structs mínimos), `alerts.rs`, `tz.rs`.
- **Medición**: `src-tauri/examples/perf_scan.rs` y `benches/common/mod.rs` (huella y tramos de refresco), test de equivalencia con golden en `src-tauri/tests/`.
- **Dependencias**: ninguna nueva (`std::thread::scope`, `std::sync::mpsc`, `serde`, `anyhow` y `tracing` ya presentes).
- **Sin impacto** en el esquema SQLite, la API de comandos, el servidor MCP (sigue llamando a `ingest::scan_all`, que mantiene su firma), el frontend ni los fixtures existentes.
- **Fuera de alcance** (fases posteriores): conexiones de lectura separadas, tablas de resumen, caché de resultados, índices para consultas, persistencia en disco, debounce/relecturas dirigidas del vigilante y eventos `data:changed`, frontend.
