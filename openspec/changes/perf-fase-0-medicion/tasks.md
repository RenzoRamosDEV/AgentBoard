# Tasks

Comandos de Rust dentro de distrobox: `distrobox enter agentburn-dev -- bash -lc 'export PATH=$HOME/.cargo/bin:$PATH; cd src-tauri && <cmd>'`.
TDD: en cada tarea que cambia comportamiento, primero el test en rojo, luego el código mínimo en verde y después refactor.

## 1. Perfiles de compilación

- [x] 1.1 Escribir `src-tauri/tests/build_profiles.rs` (lee `Cargo.toml` como texto) que exige en `[profile.release]` `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`, `panic = "abort"`, `strip = true` y en `[profile.profiling]` `inherits = "release"`, `debug = true`, `strip = false`; verificar que `cargo test --test build_profiles` falla (rojo)
- [x] 1.2 Añadir ambas secciones a `src-tauri/Cargo.toml` (D7); verificar `cargo test --test build_profiles` en verde y que `cargo build --release -p agentboard-mcp` compila
- [x] 1.3 Revisar `unwrap`/`expect` en los hilos de escaneo y vigilante (riesgo de `panic = "abort"`, design.md); verificar que no hay ninguno nuevo en rutas de ingesta (`git diff` sin `unwrap()` añadidos) y anotar los existentes para `BASELINE.md`

## 2. Inicialización de trazas

- [x] 2.1 Añadir `tracing` y `tracing-subscriber` (feature `env-filter`, `fmt`) a `[dependencies]` del crate `agentboard`; verificar con `cargo build`
- [x] 2.2 Tests en rojo en `src/telemetry.rs` (escritor en búfer + `tracing::subscriber::with_default`): sin variable un span `debug` y un `info!` no escriben nada y un `warn!` sí; con `debug` el cierre de un span escribe su nombre y `time.busy`; un valor inválido se comporta como `warn`; el escritor por defecto es stderr (nunca stdout); verificar que `cargo test telemetry` falla
- [x] 2.3 Implementar `filter_from`, `subscriber_with_writer` e `init()` idempotente leyendo `AGENTBOARD_LOG` (D1, D2); verificar `cargo test telemetry` en verde
- [x] 2.4 Llamar a `telemetry::init()` al principio de `lib.rs::run` y de `mcp/src/main.rs`; verificar que `cargo test -p agentboard-mcp` sigue en verde y que con `AGENTBOARD_LOG=debug` stdout del MCP solo contiene JSON-RPC (prueba manual con una petición `initialize` por stdin)
- [x] 2.5 Documentar `AGENTBOARD_LOG` (valores de ejemplo y nivel por defecto) en `CONTRIBUTING.md` y `docs/wiki/Arquitectura.md`; verificar que el ejemplo documentado funciona con `cargo run --example scan`

## 3. Sustituir `eprintln!` por avisos/errores de trazas

- [x] 3.1 Test en rojo: `scan_all` sobre un directorio temporal con un archivo ilegible (p. ej. permisos `000` en unix) bajo un subscriber de test en nivel `warn` captura un aviso con la ruta; verificar que falla mientras exista el `eprintln!`
- [x] 3.2 Sustituir los `eprintln!` de `ingest.rs`, `lib.rs` (2), `watcher.rs` (2), `tray.rs` y `mcp/src/main.rs` por `warn!`/`error!` según D2, conservando el texto y la ruta como campo; verificar el test 3.1 en verde y que `grep -rn 'eprintln!' src-tauri/src src-tauri/mcp/src` no devuelve nada

## 4. Spans de instrumentación

- [x] 4.1 Test en rojo (subscriber de test en `debug` que registra nombres y campos de spans cerrados): ingerir el fixture `tests/fixtures/claude_code` con `ClaudeCode::with_roots` produce spans `scan_all`, `walk`, `ingest_file` (con `bytes`, `lines`, `records` > 0) e `ingest.tx`; verificar que falla
- [x] 4.2 Añadir los spans de ingesta de D3 (`scan_all`, `walk`, `ingest_file`, `ingest.tx`) con `skip_all` y campos registrados al final; verificar el test 4.1 en verde
- [x] 4.3 Test en rojo: llamar a `queries::summary`, `queries::breakdown`, `queries::timeseries`, `queries::timeseries_by`, `insights::activity`, `insights::activity_daily`, `sessions::list_sessions` sobre una base sembrada genera un span por consulta; verificar que falla
- [x] 4.4 Instrumentar las consultas de `queries.rs`, `sessions.rs`, `insights.rs` (y `findings.rs` si la invoca un comando) llamadas desde comandos, y cada `#[tauri::command]` de `commands.rs` con span `cmd.<nombre>` (D3); verificar el test 4.3 en verde y `cargo build`
- [x] 4.5 Envolver las dos emisiones de `ingest://done` (escaneo inicial en `lib.rs` y callback del vigilante) en el span `emit.ingest_done` con campo `origen`; verificar con `cargo build` y, en la app (`npm run tauri dev` con `AGENTBOARD_LOG=debug`), que stderr muestra spans de escaneo, archivo, inserción, comando y emisión con duración
- [x] 4.6 Test de no regresión: el mismo escaneo de `tests/fixtures` con subscriber `debug` y sin subscriber da `ScanStats` y `summary` idénticos; verificar en verde junto con `cargo test` completo

## 5. Generador de fixtures sintéticos

- [x] 5.1 Escribir `scripts/test_gen_fixtures.py` (`unittest`) en rojo: con tamaño `2m` y la misma semilla dos ejecuciones dan árboles idénticos byte a byte; tamaño total dentro de ±5 %; otra semilla da contenido distinto; existen las carpetas de los cinco agentes con la estructura de `tests/fixtures`; todas las líneas son JSON válido; existe la sesión grande de Claude Code (~5 MB) en tamaños ≥ `50m`; verificar `python3 -m unittest scripts/test_gen_fixtures.py` falla
- [x] 5.2 Implementar `scripts/gen-fixtures.py` según D4 (stdlib, semilla, `50m|200m|1g|<n>{k,m,g}`, salida por defecto `src-tauri/target/perf-fixtures/<tamaño>/`, sin texto real); verificar los tests de 5.1 en verde
- [x] 5.3 Test de integración Rust en `src-tauri/tests/` que, si `python3` está disponible, genera `2m` en un directorio temporal y lo escanea con los cinco proveedores vía `with_roots`: `errors == 0` y llamadas > 0 por agente; si no hay `python3`, se salta con mensaje; verificar en verde
- [x] 5.4 Generar `50m`, `200m` y `1g`; verificar tamaños (±5 %) con `du -sb` y anotar los tiempos de generación

## 6. Benchmarks criterion

- [x] 6.1 Añadir `criterion` como dev-dependency y las entradas `[[bench]]` (`harness = false`) para `parse_line`, `ingest_file` y `summary_queries`; verificar `cargo bench --no-run`
- [x] 6.2 Implementar `benches/common` (localizar/generar el fixture `50m`, proveedores con `with_roots`, mensaje claro y salida sin error si no hay fixture) (D5); verificar ejecutando un bench con el fixture borrado y comprobando el mensaje
- [x] 6.3 Implementar `benches/parse_line.rs` (una línea representativa por proveedor JSONL); verificar `cargo bench --bench parse_line`
- [x] 6.4 Implementar `benches/ingest_file.rs` (sesión de ~5 MB, base nueva por iteración, `Throughput::Bytes`); verificar `cargo bench --bench ingest_file`
- [x] 6.5 Implementar `benches/summary_queries.rs` (5 consultas de D5 con filtro `from/to` fijo de 30 días); verificar `cargo bench --bench summary_queries`

## 7. Binario de medición

- [ ] 7.1 Tests en rojo de las piezas puras de `perf_scan` (en el módulo común): parseo de argumentos (falta `--fixtures` → error; valores por defecto), lectura de `VmHWM` desde un texto de ejemplo, y el guard que borra el archivo de refresco al soltarse; verificar que fallan
- [ ] 7.2 Implementar `src-tauri/examples/perf_scan.rs` según D6 (fases `cold|live|queries|all`, `--live-lines`, `--repeat`, llamada completa del Resumen, RSS pico, sin `providers::all()`); verificar los tests de 7.1 en verde y `cargo run --release --example perf_scan -- --fixtures target/perf-fixtures/50m` imprime todas las fases, sin errores de escaneo, y que tras la ejecución no queda ningún `perf-live-*.jsonl`

## 8. Perfilado y línea base

- [ ] 8.1 Instalar herramientas en distrobox (`sudo dnf install perf`, `cargo install flamegraph`) y comprobar `perf stat true`; si falla, anotar motivo y `perf_event_paranoid` para el fallback (D8)
- [ ] 8.2 Generar `docs/perf/cold-scan.svg` (fixture `1g`, `--phase cold`) y `docs/perf/live-refresh.svg` (`--phase live --repeat 200`) con el perfil `profiling`; verificar que los SVG abren y contienen símbolos de `agentboard_lib` (o, en fallback, documentar el comando exacto)
- [ ] 8.3 Medir con `perf_scan` en release (sin `AGENTBOARD_LOG`) escaneo frío 50 MB/200 MB/1 GB, refresco en vivo, las 5 consultas y la llamada completa del Resumen, y memoria pico; ejecutar `cargo bench` completo; verificar que cada cifra tiene comando reproducible
- [ ] 8.4 Redactar `docs/perf/BASELINE.md` (D9) con entorno, comandos, tablas de tiempos y memoria, resultados de criterion, enlaces a los SVG, riesgos de `panic = "abort"` de 1.3 y el ranking top-5 de cuellos de botella, cada uno con evidencia (% del flamegraph, span o bench); verificar que los comandos documentados se ejecutan tal cual

## 9. Comprobaciones finales

- [ ] 9.1 `cargo fmt --check`, `cargo clippy -- -D warnings` (y `cargo clippy --benches --examples --tests -- -D warnings` para el código nuevo) y `cargo test` (incluido `-p agentboard-mcp`) en verde
- [ ] 9.2 `python3 -m unittest scripts/test_gen_fixtures.py`, `npm test` y `npm run build` en verde
- [ ] 9.3 Verificar que no hay cambios de comportamiento: la app arranca sin `AGENTBOARD_LOG` sin trazas en stderr, el Resumen muestra las mismas cifras que antes del cambio y el servidor MCP responde a sus herramientas

## Workflow follow-up

- Archivar el cambio tras la revisión y sincronizar la capacidad `rendimiento` en `openspec/specs/`.
