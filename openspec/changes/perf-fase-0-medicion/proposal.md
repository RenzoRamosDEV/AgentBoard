# Proposal

## Why

Antes de optimizar la ingesta, SQLite, el vigilante o la interfaz hace falta saber dónde se va realmente el tiempo. Hoy no hay trazas, ni datos de prueba de tamaño realista, ni benchmarks, ni un perfil de compilación pensado para medir: cualquier optimización sería a ciegas. Esta fase 0 deja números de referencia reproducibles y el ranking de los 5 cuellos de botella reales, para que las fases siguientes (1–4) optimicen con evidencia ("medir antes de optimizar").

## What Changes

- **Trazas estructuradas** en el backend: spans con duración (y bytes/líneas/registros donde aplique) en el recorrido de carpetas, la ingesta por archivo, la transacción de inserción, cada comando Tauri, cada consulta agregada que invocan los comandos y la emisión de `ingest://done`.
- **Activación por variable de entorno** `AGENTBOARD_LOG` (sintaxis de filtro por niveles/módulos, p. ej. `AGENTBOARD_LOG=debug`). Sin la variable el nivel es `warn`: no se emiten spans y los avisos/errores siguen saliendo por stderr como hoy.
- Los `eprintln!` de error existentes pasan a avisos/errores del sistema de trazas, sin cambiar lo que ve el usuario por defecto (siguen en stderr). Aplica también al servidor MCP, que reutiliza la misma librería.
- **Generador de fixtures sintéticos** `scripts/gen-fixtures.py` (Python 3, sin dependencias): logs realistas de Claude Code, Codex, Copilot, Cursor y Gemini en el mismo formato que `tests/fixtures/<agente>`, tamaños `50m`, `200m` y `1g`, con semilla fija (determinista) y sin texto real.
- **Benchmarks `criterion`** en `src-tauri/benches/`: parseo de una línea por proveedor JSONL, ingesta de un archivo de ~5 MB de Claude Code y las 5 consultas más usadas del Resumen sobre una base cargada con el fixture de 50 MB.
- **Binario de medición** `src-tauri/examples/perf_scan.rs`: escaneo frío sobre una carpeta de fixtures (raíces inyectadas, nunca los logs reales), refresco en vivo y consultas del Resumen, imprimiendo tiempos y memoria pico (RSS). Es el objetivo de los flamegraphs.
- **Perfiles de compilación**: `release` con LTO completo, una unidad de codegen, `opt-level = 3`, `panic = "abort"` y `strip`; perfil `profiling` que hereda de `release` con símbolos de depuración y sin `strip`.
- **Documentación de la línea base** en `docs/perf/`: flamegraphs `cold-scan.svg` y `live-refresh.svg` y `BASELINE.md` con entorno, comandos de reproducción, tiempos, memoria pico, resultados de criterion y el ranking de los 5 cuellos de botella con evidencia.
- Sin cambios de comportamiento visibles para el usuario. Ninguna optimización del código de ingesta, SQLite, vigilante o UI (queda para las fases 1–4).

## Capabilities

### New Capabilities
- `rendimiento`: observabilidad del rendimiento del backend (trazas activables por entorno y su comportamiento por defecto) y la medición reproducible de referencia (datos sintéticos deterministas, medición aislada de los logs reales y línea base documentada).

### Modified Capabilities
<!-- Ninguna: el escaneo, el vigilante, el almacenamiento y las vistas mantienen sus requisitos. -->

## Impact

- **Código**: `src-tauri/src/lib.rs` (inicialización de trazas, span de emisión), `ingest.rs`, `watcher.rs`, `commands.rs`, `queries.rs`, `sessions.rs`, `insights.rs`, `tray.rs` (spans y sustitución de `eprintln!`), módulo nuevo de inicialización de trazas, `src-tauri/mcp/src/main.rs` (inicializa trazas para conservar los errores por stderr).
- **Dependencias**: `tracing`, `tracing-subscriber` (feature `env-filter`) en el crate `agentboard`; `criterion` como dev-dependency.
- **Compilación**: secciones `[profile.release]` y `[profile.profiling]` en `src-tauri/Cargo.toml` (raíz del workspace; afectan también al binario MCP). `panic = "abort"` cambia el modo de fallo ante un pánico (ver design.md).
- **Archivos nuevos**: `scripts/gen-fixtures.py` y su test, `src-tauri/benches/*`, `src-tauri/examples/perf_scan.rs`, `docs/perf/` (SVG y `BASELINE.md`).
- **Sin impacto** en la API de comandos, el esquema de la base, el protocolo MCP ni el frontend.
