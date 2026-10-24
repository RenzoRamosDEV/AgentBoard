# Tasks

Comandos de Rust dentro de distrobox: `distrobox enter agentburn-dev -- bash -lc 'export PATH=$HOME/.cargo/bin:$PATH; cd src-tauri && <cmd>'`.
TDD: en cada tarea que cambia comportamiento, primero el test en rojo (verificar que falla), luego el código mínimo en verde y después refactor. El test de equivalencia (1.3) MUST seguir en verde tras cada tarea.
**Medición (M)**: `cargo build --release --example perf_scan` y tres ejecuciones de `./target/release/examples/perf_scan --fixtures target/perf-fixtures/200m --phase cold` (y `--phase live --repeat 200` cuando se indique), sin `AGENTBOARD_LOG`; anotar la mediana frente al paso anterior y a `docs/perf/BASELINE.md`. **Regla (D9)**: si el paso no mejora su métrica ≥ 5 % (o, si es estructural, empeora > 5 %), se revierte y se anota el motivo en «Desviaciones durante la implementación» de design.md.

## 1. Huella de resultados y línea de partida

- [x] 1.1 Tests en rojo en `src-tauri/tests/perf_common.rs` para `common::fingerprint` (D1): dos bases con las mismas filas insertadas en distinto orden dan la misma huella; cambiar un token, un `turn_id` o el `cwd` de un proyecto cambia la huella; ids autoincrementales, `first_seen` y `last_scan` no influyen; verificar que `cargo test --test perf_common` falla
- [x] 1.2 Implementar `fingerprint` en `benches/common/mod.rs` (conteos, tokens y coste por agente; FNV-1a 64 por tabla sobre filas ordenadas por clave natural); verificar `cargo test --test perf_common` en verde
- [x] 1.3 Escribir `src-tauri/tests/equivalencia_ingesta.rs` (fixture sintético `2m` semilla 11 con los cinco proveedores JSONL, y `tests/fixtures` con todos los proveedores incluido OpenCode) contra `tests/golden/ingesta_2m.txt` y `tests/golden/ingesta_fixtures.txt`, con `UPDATE_GOLDEN=1` para grabar; verificar que falla sin golden, grabarlos **en el código actual sin optimizaciones** y verificar que pasa en verde
- [x] 1.4 Test en rojo en `src-tauri/tests/perf_common.rs`: `parse_args` acepta `--fingerprint`, y la función de resumen de refrescos devuelve la mediana de los primeros 20 y de los últimos 20 (y con menos de 40 refrescos usa las mitades); verificar que falla
- [x] 1.5 Añadir a `perf_scan` la opción `--fingerprint` (imprime la huella tras el frío) y las filas `refresco (mediana primeros 20 / últimos 20)` en la fase `live`; verificar 1.4 en verde y que `perf_scan --fixtures target/perf-fixtures/50m --phase live --repeat 40 --fingerprint` imprime ambas cosas
- [x] 1.6 Medición de partida (M) con `--fingerprint` y `--phase live --repeat 200` sobre `200m`; guardar la huella de `200m` en design.md («Desviaciones…» → «Partida») y verificar que las cifras están en el rango de `BASELINE.md`

## 2. Escritor con sentencias en caché (cuello #1)

- [x] 2.1 Test en rojo en `ingest.rs`: tras ingerir un archivo de 200 llamadas con tool uses, el número de compilaciones de sentencias (contador vía `Connection::trace`/`profile` o `sqlite3_stmt_status` en un test) no supera el número de sentencias distintas; verificar que falla
- [x] 2.2 Pasar todas las sentencias de `Writer` y de `file_state` a `prepare_cached` con SQL constantes y subir la capacidad de la caché a ≥ 32 en `db::open_in_memory` (D2); verificar 2.1 y 1.3 en verde y `cargo test --workspace`
- [x] 2.3 Medir (M, frío) y `cargo bench --bench ingest_file`; aplicar la regla D9 y anotar las cifras
- [ ] 2.4 (Opcional, D2b) Test en rojo: un archivo de Claude Code con 1 000 líneas de una misma sesión hace como mucho un `INSERT` y un `UPSERT` final sobre `sessions` y deja `started_at`, `ended_at`, `git_branch`, `project_id`, `is_subagent` y `model` iguales que antes; implementar el coalescido; verificar 2.4 y 1.3 en verde, medir (M, frío) y aplicar la regla D9

## 3. Relleno de turnos acotado (cuello #3)

- [x] 3.1 Tests en rojo en `ingest.rs`: (a) releer una sesión con llamadas sin turno añadiendo líneas sin prompts no ejecuta ningún `UPDATE` de relleno (contador de sentencias); (b) un subagente leído antes que el archivo con su turno queda asignado a ese turno; (c) un turno nuevo solo rellena filas con `ts >= ts_turno`; verificar que (a) falla y (b), (c) pasan
- [x] 3.2 Registrar en `Writer` el `ts` mínimo de los turnos aplicados por sesión y limitar `backfill_turns` a esas sesiones con `ts >= ?` (D3); verificar 3.1 y 1.3 en verde
- [x] 3.3 Medir (M, frío y `live --repeat 200`); comprobar que la mediana de los últimos 20 refrescos ≤ 1,5 × la de los primeros 20; aplicar la regla D9 y anotar las cifras

## 4. Lectura incremental robusta

- [x] 4.1 Tests en rojo en `ingest.rs`: (a) archivo rotado (misma ruta, otra identidad, igual o mayor tamaño) se relee desde 0 sin duplicar; (b) un archivo sin cambios no se abre (archivo con permisos `000` tras la primera lectura y mismo tamaño: sin error en el segundo escaneo); (c) un `file_state` precargado una vez por escaneo (un único `SELECT` de `file_state`, contado con `Connection::trace`); verificar que fallan los que correspondan
- [x] 4.2 Cargar `file_state` en un `HashMap` al inicio del escaneo, saltar sin abrir los archivos sin cambios y leer con `BufReader` + `read_until(b'\n')` desde el offset, sin consumir la última línea sin `\n` (D5); verificar 4.1, los tests existentes de lectura incremental (`linea_incompleta_espera_a_la_siguiente_pasada`, `archivo_truncado_se_relee_sin_duplicar`, documentos completos) y 1.3 en verde
- [x] 4.3 Medir (M, frío y `live --repeat 200`); aplicar la regla D9 y anotar las cifras

## 5. Escaneo paralelo determinista

- [x] 5.1 Tests en rojo: (a) escanear el fixture `2m` con 1 y con 4 hilos da la misma huella, repetido 5 veces; (b) con `N = 2`, un archivo grande y 50 pequeños el escaneo termina (sin interbloqueo) con la misma huella; (c) un archivo ilegible entre válidos cuenta un error con su ruta y el resto se importa; (d) un archivo cuya escritura falla (p. ej. registro que viola una restricción en test) deshace solo ese archivo; verificar que fallan
- [x] 5.2 Implementar el pipeline de D4 (plan → workers con `std::thread::scope`, ventana `4 × N`, `sync_channel` → escritor que reordena por índice y escribe en lotes de ~5 000 registros con `SAVEPOINT` por archivo), con el sumidero `&mut Connection` para `scan_all` (firma sin cambios) y `&Mutex<Connection>` para uso compartido; conservar `ScanStats` y spans; verificar 5.1, 1.3, `cargo test --workspace` (incluido `trazas_sin_regresion` y el MCP) en verde
- [x] 5.3 Medir (M, frío y `live --repeat 200`) con el número de hilos por defecto; ajustar lote y ventana solo si la medición lo justifica; aplicar la regla D9 y anotar las cifras

## 6. Hilo de ingesta dedicado

- [x] 6.1 Tests en rojo en `src-tauri/src/ingest_thread.rs`: (a) `request(EscaneoInicial)` ejecuta un escaneo y llama a `on_done` con sus `ScanStats`; (b) varias peticiones encoladas se agrupan en un único escaneo; (c) mientras el hilo lee y parsea, otra hebra puede bloquear el `Mutex` de la base y consultar (el escaneo no lo retiene entre lotes); (d) un error de escaneo no detiene el hilo y la siguiente petición se atiende; verificar que fallan
- [x] 6.2 Implementar `IngestThread`/`IngestHandle` con `std::sync::mpsc` y un único conjunto de proveedores (D6); verificar 6.1 en verde
- [x] 6.3 Adaptar `watcher::spawn` para encolar `Origen::Vigilante` en vez de escanear, y sus tests (`escanea_carpetas_que_aparecen_despues_de_arrancar`, `relee_al_cambiar_un_log_sin_duplicar`) para usar el hilo de ingesta; verificar en verde
- [x] 6.4 En `lib.rs`, crear el hilo de ingesta, encolar el escaneo inicial y pasar el handle al vigilante, conservando los avisos `ingest://done` (con `ScanStats` en el inicial, sin datos en el vigilante y solo si hubo registros) y el refresco de bandeja/avisos; verificar `cargo build` y, en la app (`npm run tauri dev` con `AGENTBOARD_LOG=debug`), que el escaneo inicial y una relectura emiten `emit.ingest_done` y el dashboard se actualiza
- [x] 6.5 Medir (M, frío y `live --repeat 200`; el frío de `perf_scan` no usa el hilo, así que además medir con `AGENTBOARD_LOG=debug` en la app la duración de `scan_all` del escaneo inicial frente a la partida); aplicar la regla D9 (paso estructural)

## 7. Comandos Tauri `async`

- [x] 7.1 Tests en rojo en `commands.rs` para `run_blocking` (núcleo de `with_db`): ejecuta el cierre en un hilo distinto del llamador, devuelve su resultado y formatea los errores como `{e:#}`; verificar que fallan
- [x] 7.2 Pasar todos los comandos de `commands.rs` a `async fn` con `with_db` sobre `tauri::async_runtime::spawn_blocking`, y `settings::load/save` de `get_settings`, `list_prices` y `set_settings` dentro de `spawn_blocking` (D7); verificar 7.1 en verde, `cargo build`, `npm test` y que en la app todas las vistas cargan igual (mismos nombres y respuestas de comandos)
- [x] 7.3 Con `AGENTBOARD_LOG=debug` en la app sobre el fixture `200m` (raíces de prueba) comprobar que los spans `cmd.*` se completan durante el escaneo inicial y anotar sus duraciones; aplicar la regla D9 (paso estructural)

## 8. Parseo mínimo con sonda de tipo

- [ ] 8.1 Claude Code — tests en rojo en `providers/claude_code.rs`: una línea sin `sessionId` o sin `timestamp` se descarta tras la sonda sin interpretar `message`; un prompt de usuario con escapes y de varios MB devuelve la misma intención sin copiar el texto (structs con `Cow` prestado); resultados de todos los tests existentes iguales; verificar que fallan los nuevos
- [ ] 8.2 Claude Code — implementar sonda y structs mínimos (D8); verificar 8.1 y 1.3 en verde; `cargo bench --bench parse_line` (`parse_line/claude_code`) y M (frío); aplicar la regla D9
- [ ] 8.3 Codex — derivar de su `parse_line` los tipos que no producen registros ni cambian estado; tests en rojo de descarte y de igualdad con los existentes; implementar sonda y structs mínimos; verificar tests y 1.3 en verde; `parse_line/codex` y M (frío); aplicar la regla D9
- [ ] 8.4 Copilot — igual que 8.3; verificar tests y 1.3 en verde; `parse_line/copilot` y M (frío); aplicar la regla D9
- [ ] 8.5 Cursor — igual que 8.3 (evitar el `clone` del array de bloques de `message.content`); verificar tests y 1.3 en verde; `parse_line/cursor` y M (frío); aplicar la regla D9
- [ ] 8.6 Gemini — igual que 8.3; verificar tests y 1.3 en verde; `parse_line/gemini` y M (frío); aplicar la regla D9

## 9. Sin pánicos en hilos de fondo

- [ ] 9.1 Tests en rojo en `tz.rs` y `alerts.rs`: `day_start` y `hour_start` con `i64::MIN`, `i64::MAX` y horas del salto de horario de `Europe/Madrid` no entran en pánico (valores límite: devuelven `ts` sin truncar si no hay representación local) y dan los mismos valores que hoy para entradas válidas (casos existentes + bordes de medianoche); verificar que fallan
- [ ] 9.2 Sustituir los `expect`/`unwrap` de `alerts.rs:91` y `tz.rs:37,42,49,54` (D10); verificar 9.1 en verde
- [ ] 9.3 Añadir `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]` en `ingest.rs`, `ingest_thread.rs`, `watcher.rs`, `tz.rs`, `alerts.rs` y `providers/*`; verificar `cargo clippy --workspace --all-targets -- -D warnings` sin errores

## 10. Resultados y cierre

- [ ] 10.1 Mutación sobre el diff de la fase (`cargo mutants --in-diff` limitado a los archivos cambiados, tiempo máximo 3 min por ejecución): puntuación > 95 % o justificar cada mutante superviviente en design.md
- [ ] 10.2 Medición final completa con los comandos de «Reproducción» de `docs/perf/BASELINE.md` (frío y vivo en 50 MB/200 MB/1 GB, `--repeat 200` en 200 MB, RSS pico, `cargo bench`); verificar `perf_scan --fingerprint` sobre `200m` igual a la huella de partida (1.6) y los presupuestos del spec: frío 200 MB < 10 s, mediana de 200 refrescos < 200 ms, últimos 20 ≤ 1,5 × primeros 20
- [ ] 10.3 Añadir a `docs/perf/BASELINE.md` la sección «Después de la Fase 1» (entorno y carga de la máquina, tabla antes/después por métrica, flamegraph `docs/perf/cold-scan-fase1.svg` del frío 1 GB, pasos conservados y revertidos con su motivo, estado frente a los presupuestos y seguimientos para la Fase 2: comprobaciones de clave foránea, índice parcial de turnos); verificar que todos los comandos citados se ejecutan tal cual
- [ ] 10.4 Comprobaciones finales: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm test`, `npm run build`; verificar que todo pasa
- [ ] 10.5 Completar «Desviaciones durante la implementación» de design.md con todo lo que cambió respecto a D1–D10; verificar `openspec validate perf-fase-1-ingesta --strict`

## Workflow follow-up

- Archivar el cambio cuando la revisión esté satisfecha y sincronizar `openspec/specs/rendimiento/spec.md`.
