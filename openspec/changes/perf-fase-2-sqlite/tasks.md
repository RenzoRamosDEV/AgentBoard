# Tasks

Comandos de Rust dentro de distrobox: `distrobox enter agentburn-dev -- bash -lc 'export PATH=$HOME/.cargo/bin:$PATH; cd src-tauri && <cmd>'`.
TDD: en cada tarea que cambia comportamiento, primero el test en rojo (verificar que falla), luego el código mínimo en verde y después refactor. Los golden de ingesta (`tests/equivalencia_ingesta.rs`) y de consultas (1.2) MUST seguir en verde tras cada tarea, sin regrabarlos.
**Medición (MQ)**: `cargo build --release --example perf_scan` y tres ejecuciones de `./target/release/examples/perf_scan --fixtures target/perf-fixtures/200m --phase queries` (y `1g` cuando se indique), sin `AGENTBOARD_LOG`; anotar la mediana del Resumen completo (frío y repetido) y de cada consulta frente al paso anterior. **(MF)**: lo mismo con `--phase cold` (y `--phase live --repeat 200` cuando se indique). **Regla (D13)**: si el paso no mejora su métrica ≥ 5 % (o, si es estructural, empeora > 5 %), se revierte y se anota el motivo en «Desviaciones durante la implementación» de design.md.

## 1. Golden de consultas y línea de partida

- [x] 1.1 Test en rojo en `src-tauri/tests/perf_common.rs` para el comparador del golden de consultas (D1): enteros y cadenas exactos, reales iguales con diferencia ≤ 1e-6 y distintos con 2e-6, claves de más o de menos fallan; y para el volcado canónico (orden estable, reales redondeados en el hash); verificar que falla
- [x] 1.2 Implementar volcado y comparador en `benches/common/mod.rs` y escribir `src-tauri/tests/equivalencia_consultas.rs` (fixture `2m` semilla 11 y `tests/fixtures`; consultas y filtros de D1; zona `Europe/Madrid`, `now` fijo) contra `tests/golden/consultas_2m.txt` y `consultas_fixtures.txt` con `UPDATE_GOLDEN=1`; verificar que falla sin golden, grabarlos **en el código actual sin optimizaciones** y verificar que pasa
- [x] 1.3 Test en rojo en `tests/perf_common.rs`: `parse_args` acepta `--query-fingerprint`, `--cache <ruta>` y las fases `warm` y `contention`; verificar que falla; implementar en `perf_scan` solo `--query-fingerprint` y el Resumen repetido de `--phase queries` (las demás opciones se completan en 8.4 y 10.6) y verificar en verde
- [x] 1.4 Medición de partida (MQ en `200m` y `1g`, MF en `200m`) con `--query-fingerprint` sobre `200m`; guardar la huella y las cifras en design.md («Desviaciones…» → «Partida») y verificar que están en el rango de «Después de la Fase 1» de `docs/perf/BASELINE.md`

## 2. Coste precalculado y vista sin subconsultas (D6)

- [x] 2.1 Tests en rojo en `pricing.rs`/`db.rs`: (a) tras insertar llamadas con SQL directo, `calls.cost_usd`, `cache_savings_usd` y `has_price` valen lo mismo que la fórmula de la vista 0004 (precio con fecha, coste reportado, sin precio, caché de 1 h); (b) insertar, borrar o cambiar un precio recalcula solo las llamadas de ese modelo (contador de filas actualizadas con `db::trace`); (c) `apply_overrides` con los mismos precios no modifica `prices` ni ninguna llamada; con un precio por defecto distinto recalcula solo ese modelo; (d) `call_costs` no contiene `SELECT *` ni subconsultas (`sqlite_master`); verificar que fallan
- [x] 2.2 Migración `0005` con columnas de coste, disparadores de `calls` y `prices`, vista `call_costs` con columnas explícitas, y `apply_overrides` por diferencias (D6); verificar 2.1, los tests existentes de `pricing` sin cambiar sus aserciones, `base_de_fase_0_migra_sin_perder_datos`, 1.2 y `equivalencia_ingesta` en verde
- [x] 2.3 Medir (MQ en `200m`/`1g` y MF); aplicar la regla D13 (si el frío empeora > 5 %, probar la alternativa de coste en el `UPSERT` antes de revertir) y anotar las cifras

## 3. Primera llamada de la sesión sin `ROW_NUMBER` (D6)

- [x] 3.1 Tests en rojo en `queries.rs`: (a) `sessions.first_call_id` es la llamada principal con menor `(ts, rowid)` y cambia si llega una anterior (también de otro archivo); (b) llamadas de subagente (`is_sidechain`) nunca son la primera; (c) el SQL de `breakdown` no contiene `ROW_NUMBER`; verificar que fallan
- [x] 3.2 Mantener `first_call_id` (en la función de refresco que crece en 5.2; aquí basta con recalcularlo para las sesiones tocadas al final de cada lote y en `testdata::seed`) y cambiar el CTE `firsts` por el `JOIN` de D6; verificar 3.1 y 1.2 en verde (si `overhead_tokens` difiere por empates de `ts`, aplicar el riesgo de design.md)
- [x] 3.3 Medir (MQ en `200m`/`1g`); aplicar la regla D13 y anotar las cifras

## 4. Día local precalculado (D6)

- [x] 4.1 Tests en rojo: (a) el `Writer` guarda `day = YYYYMMDD` local de la zona de la caché, también en los bordes de medianoche y en los dos cambios de hora de `Europe/Madrid`; (b) un `UPSERT` que adelanta `ts` de una llamada a otro día cambia su `day`; (c) filas sin `day` (SQL directo) lo reciben en el refresco; (d) abrir una base con `cache_meta.tz` distinta de la zona del sistema vacía los días y los recalcula; verificar que fallan
- [x] 4.2 Columna `day` en `calls` y `tool_calls`, `cache_meta.tz`, cálculo en el `Writer` y en el refresco (D6); verificar 4.1, 1.2 y `equivalencia_ingesta` en verde
- [x] 4.3 Medir (MF); aplicar la regla D13 (estructural) y anotar las cifras

## 5. Agregados diarios incrementales (D7)

- [x] 5.1 Tests en rojo en un módulo nuevo `derived.rs`: (a) tras insertar, actualizar (`UPSERT` que cambia tokens o `ts`) y borrar llamadas y herramientas, `refresh` deja `daily_stats`/`tool_stats` iguales a reconstruirlos desde cero (test de propiedad con secuencias aleatorias de semilla fija, incluidos cambios de precio); (b) un lote que toca una sesión y un día solo reescribe esa clave (contador de filas); (c) `dirty_keys` queda vacía tras `refresh`; verificar que fallan
- [x] 5.2 Tablas, disparadores de claves sucias y `derived::refresh` (D7), llamado al final de cada lote del `Writer`, en `apply_overrides` y en `queries::testdata::seed`/helpers de `pricing`; verificar 5.1, 1.2 y `cargo test --workspace` en verde
- [x] 5.3 Tests en rojo en `queries.rs` de la ruta de consulta: con `from` en medianoche local y la zona de la caché se leen agregados (el plan no toca `calls`), con `from` a mitad de día o con otra zona se usa el detalle, y ambas rutas dan lo mismo sobre `testdata` y sobre el fixture `2m` (equivalencia por partición: todo, días completos, mitad de día, `to` en el futuro, periodo vacío, filtro de agente + proyecto, `no_project`); verificar que fallan
- [x] 5.4 Pasar a agregados `summary` (salvo `burn rate`), `timeseries(day)`, `timeseries_by` (5 dimensiones), `breakdown(agent|project|branch|model|tool|mcp)`, `list_agents` y `list_projects` (D7); verificar 5.3, 1.2 (también el servidor MCP: `cargo test -p agentboard-mcp`) en verde
- [ ] 5.5 Medir (MQ en `200m`/`1g` y MF con `--phase live --repeat 200`); aplicar la regla D13 (si el frío empeora > 5 % por los disparadores, probar la alternativa de claves sucias en el `Writer` antes de revertir) y anotar las cifras

## 6. Índices revisados con `EXPLAIN QUERY PLAN` (D9)

- [x] 6.1 Test en rojo `consultas_usan_indices` en `queries.rs`: para cada consulta del Resumen con filtro de 30 días, con agente y con proyecto, y para el relleno de turnos y el refresco de agregados, el plan no contiene `SCAN calls`/`SCAN tool_calls` sin índice; verificar qué consultas fallan y anotar sus planes en design.md
- [x] 6.2 Añadir los índices de D9 que el plan pida (y quitar los que no use ninguna consulta); verificar 6.1, 1.2 y `equivalencia_ingesta` en verde
- [ ] 6.3 Medir (MQ en `200m`/`1g` y MF con `--phase live --repeat 200`); aplicar la regla D13 por índice y anotar las cifras

## 7. Claves foráneas fuera del escritor (D3)

- [x] 7.1 Tests en rojo en `ingest.rs`: (a) con `foreign_keys=OFF` una llamada, herramienta, turno o evento de una sesión inexistente sigue fallando y deshace solo su archivo (el test existente `un_fallo_al_escribir_deshace_solo_ese_archivo` más uno por tipo de registro); (b) `PRAGMA foreign_key_check` vacío tras escanear `tests/fixtures` y el fixture `2m`; (c) la comprobación de sesión hace como mucho una búsqueda por sesión y archivo (contador); verificar que fallan los que correspondan
- [x] 7.2 Desactivar `foreign_keys` y añadir la comprobación memorizada en el `Writer`; verificar 7.1, 1.2 y `equivalencia_ingesta` en verde
- [ ] 7.3 Medir (MF y `--phase live --repeat 200`); aplicar la regla D13 (revertir a `foreign_keys=ON` si no mejora ≥ 5 %) y anotar las cifras

## 8. Escritor único por lotes y lectores separados (D4, D5)

- [x] 8.1 Tests en rojo en `ingest_thread.rs`: (a) `Job::AplicarPrecios` se atiende entre lotes de un escaneo en curso y responde por su canal; (b) un lote se confirma al llegar a 5 000 filas o a 200 ms (proveedor de prueba lento); (c) `DataGeneration` sube tras cada `COMMIT` y no antes; (d) un error de un trabajo no detiene el hilo; verificar que fallan
- [x] 8.2 Hilo de ingesta dueño de la conexión de escritura, trabajos nuevos, lotes por filas o tiempo y contador de generación (D4); `set_settings` aplica precios con `Job::AplicarPrecios`; verificar 8.1, los tests existentes de `ingest_thread` y `watcher`, y `cargo test --workspace` en verde
- [x] 8.3 Tests en rojo en `db.rs`: (a) con la base en archivo WAL, una consulta larga en un lector no impide que el escritor confirme un lote y una consulta durante un lote ve el estado anterior completo; (b) en memoria con caché compartida, un lector espera al lote en curso (sin `SQLITE_LOCKED`) y ve el lote entero; (c) los lectores no pueden escribir (`query_only`); verificar que fallan
- [ ] 8.4 `ReadPool` (2 conexiones, D5), feature `unlock_notify` de `rusqlite`, PRAGMAs de D3 por tipo de conexión; `AppState` con `RwLock<Arc<ReadPool>>`; comandos (`with_reader`), `alerts::refresh` y `tray::refresh` leen del pool; `perf_scan --phase contention`; verificar 8.3, `cargo test --workspace`, `npm test` y en la app (`npm run tauri dev` sobre el fixture `200m` con `AGENTBOARD_LOG=debug`) que las vistas cargan y que `emit.ingest_done` del vigilante llega mientras la interfaz consulta
- [ ] 8.5 Medir (`--phase contention` sobre `200m`: mediana del refresco con y sin el Resumen en bucle; MF con `--phase live --repeat 200`); comprobar que el refresco con lecturas no supera 1,1 × el refresco sin ellas; aplicar la regla D13 (estructural) y anotar las cifras

## 9. Caché de resultados (D8)

- [ ] 9.1 Tests en rojo en `result_cache.rs`: (a) la misma clave y generación devuelve el mismo `Arc` sin recalcular (contador); (b) otra generación vacía la caché; (c) un resultado calculado mientras cambió la generación no se guarda; (d) las consultas que dependen de la hora cambian de clave al cambiar el minuto; (e) al pasar de 256 entradas se vacía (valores límite 255/256/257); (f) filtros equivalentes con distinto orden de campos JSON comparten clave; verificar que fallan
- [ ] 9.2 Implementar `ResultCache` (D8) y usarla en los comandos del Resumen y de los apartados; verificar 9.1, `cargo test --workspace` y `npm test` en verde
- [ ] 9.3 Medir (MQ en `200m`: Resumen repetido); aplicar la regla D13 y anotar las cifras

## 10. Caché en disco, validación y reconstrucción (D2)

- [ ] 10.1 Tests en rojo en `db.rs` (carpetas temporales, nunca la de datos real): (a) `Storage::open(Disco)` crea `cache.sqlite` con WAL y, en Unix, permisos `0600` (también `-wal`/`-shm`) y carpeta `0700`; (b) reabrir conserva datos y `file_state` y un segundo escaneo sin cambios no lee ningún archivo; (c) archivo con bytes basura, base truncada y `user_version` distinta se borran y se recrean vacíos sin error; (d) carpeta sin permiso de escritura → modo memoria con aviso; (e) `remove_legacy_db` borra `agentboard.db`/`agentburn.db` y sus auxiliares pero conserva `cache.sqlite` y la carpeta; verificar que fallan
- [ ] 10.2 Implementar `Storage`, `CacheMode`, validación y `remove_legacy_db` (D2); `lib.rs` abre la caché según `settings.memory_only`; verificar 10.1 y `cargo test --workspace` en verde
- [ ] 10.3 Test en rojo: un error `SQLITE_CORRUPT` simulado en el escritor o en un lector encola `Job::Reconstruir` y la app sigue; verificar que falla; implementarlo y verificar en verde
- [ ] 10.4 Ejecutar 1.2 también en modo disco (parámetro del test) y verificar que ambos modos dan el golden
- [ ] 10.5 Medir MF con `--cache` sobre `200m` (presupuesto < 10 s); aplicar la regla D13 (estructural) y anotar las cifras
- [ ] 10.6 Completar `perf_scan --cache` y `--phase warm` (D13); medir el arranque con 200 MB ya indexados (tres ejecuciones) y verificar < 1,5 s hasta los datos del Resumen; anotar las cifras

## 11. Caché fiel a los logs (D10)

- [ ] 11.1 Tests en rojo en `ingest.rs`: (a) escanear A∪B, borrar B y reescanear da la misma huella (`common::fingerprint`) que escanear solo A en una base nueva; (b) igual con una sesión repartida entre A y B (sus `started_at`/`ended_at` vuelven a los de A); (c) igual con B rotado (otra identidad) y con B truncado; (d) un archivo sin cambios no se relee tras una limpieza que no le afecta; (e) la limpieza rehace los agregados de las sesiones afectadas; verificar que fallan
- [ ] 11.2 `file_state` con `id` estable, tabla `session_files` y limpieza al planificar (D10); verificar 11.1, 1.2, `equivalencia_ingesta` y los tests de rotación y truncado existentes en verde
- [ ] 11.3 Medir (MF y `--phase live --repeat 200`); aplicar la regla D13 (estructural) y anotar las cifras

## 12. Ajustes «Solo memoria» y «Reconstruir caché» (D12)

- [ ] 12.1 Tests en rojo en Rust: (a) `Settings` sin `memory_only` en el JSON carga `false` y se guarda y relee; (b) `Job::Reconstruir(Memoria)` borra la caché en disco y deja los datos en memoria con la misma huella; (c) `Job::Reconstruir(Disco)` crea la caché y la rellena; (d) durante la reconstrucción las consultas responden sin error; (e) con `memory_only` activo al arrancar se borra una `cache.sqlite` que haya quedado; verificar que fallan
- [ ] 12.2 Implementar `memory_only`, comando `rebuild_cache`, la reconstrucción en `set_settings` y el registro del comando en `lib.rs` (D12); verificar 12.1 y `cargo test --workspace` en verde
- [ ] 12.3 Tests en rojo del frontend (`npm test`): la pestaña «Datos» de `SettingsDialog` muestra el interruptor con el valor de los ajustes, guardarlo llama a `saveSettings` con `memoryOnly`, el botón llama a `rebuildCache` y queda deshabilitado hasta `ingest://done`; `i18n` tiene los textos nuevos en `en`, `pt` y `fr`; verificar que fallan
- [ ] 12.4 Implementar la pestaña, `api.rebuildCache`, `memoryOnly` en `api.ts`, `App.tsx` y `src/dev/mock.ts`, y los textos en `i18n.ts`; verificar 12.3, `npm test`, `npm run build` y en la app que activar «Solo memoria» borra `~/.local/share/agentboard/cache.sqlite` (con `HOME` de prueba) y que «Reconstruir caché» relee y actualiza el panel

## 13. Documentación

- [ ] 13.1 Actualizar `README.md`, `CONTRIBUTING.md` y `docs/wiki/{Home,Arquitectura,Funcionalidad,Servidor-MCP}.md` según D14 (filosofía «solo lectura de los logs; caché local reconstruible», ruta y permisos de la caché, cómo borrarla, «Solo memoria», «Reconstruir caché», escritor/lectores/agregados en la arquitectura, MCP sigue en memoria); verificar con `grep -rn -i "sin persistencia\|en memoria" README.md CONTRIBUTING.md docs/wiki` que no queda ninguna afirmación contradictoria

## 14. Resultados y cierre

- [ ] 14.1 Mutación sobre el diff de la fase (`cargo mutants --in-place --in-diff <diff> --cargo-arg=--lib`, cada ejecución cortada a los 3 min con `SIGINT` y `git status` limpio después; ejecuciones dirigidas a `derived.rs`, `result_cache.rs`, `Storage`, la limpieza de D10, `apply_overrides` y la ruta de consulta): puntuación > 95 % sobre los mutantes examinados o justificar cada superviviente en design.md
- [ ] 14.2 Medición final con los comandos de «Reproducción» de `docs/perf/BASELINE.md` más `--phase warm`, `--phase contention` y `--cache` (frío y vivo en 50 MB/200 MB/1 GB, consultas en 200 MB/1 GB, arranque con caché, RSS pico y tamaño de `cache.sqlite`, `cargo bench`); verificar la huella de ingesta y `--query-fingerprint` de `200m` iguales a las de partida (1.4) y los presupuestos del spec: arranque con 200 MB indexados < 1,5 s, Resumen 200 MB < 300 ms en frío y < 100 ms en caché, cada consulta del Resumen en 1 GB < 300 ms, frío 200 MB en disco < 10 s, refresco con lecturas ≤ 1,1 × sin ellas
- [ ] 14.3 Añadir a `docs/perf/BASELINE.md` la sección «Después de la Fase 2» (entorno y carga, tabla antes/después por métrica incluidas consultas y arranque, pasos conservados y revertidos con su motivo, estado frente a los presupuestos, seguimientos para la Fase 3) y actualizar «Reproducción» con las fases nuevas; verificar que todos los comandos citados se ejecutan tal cual
- [ ] 14.4 Comprobaciones finales: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm test`, `npm run build`; verificar que todo pasa
- [ ] 14.5 Completar «Desviaciones durante la implementación» de design.md con todo lo que cambió respecto a D1–D14; verificar `openspec validate perf-fase-2-sqlite --strict`

## Workflow follow-up

- Archivar el cambio cuando la revisión esté satisfecha y sincronizar `openspec/specs/{almacenamiento-local,ajustes,rendimiento}/spec.md`.
