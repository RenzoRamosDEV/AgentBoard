# Línea base de rendimiento (Fase 0)

Medición de referencia del backend de AgentBoard antes de cualquier optimización. Todas las
cifras salen de logs **sintéticos** (`scripts/gen-fixtures.py`), nunca de los logs reales.
Las fases 1–4 deben compararse contra este documento.

## Entorno

| Campo | Valor |
|---|---|
| CPU | AMD Ryzen 5 Pro 7535U, 6 núcleos / 12 hilos, hasta 4,63 GHz (portátil, perfil `balanced`) |
| RAM | 14 GiB (+ 7 GiB swap) |
| SO | Bazzite (kernel 7.2.9) · contenedor distrobox `agentburn-dev` Fedora 44 |
| Rust | rustc 1.98.1 · cargo 1.98.1 |
| Python | 3.14.7 (generador) |
| perf / flamegraph | perf 7.2.11 · cargo-flamegraph 0.6.14 · `perf_event_paranoid = 2` |
| Commit medido | `cd5d4fa` (rama `perf/rendimiento-fluido`) |
| Perfil | `release` (LTO fat, 1 codegen unit, `opt-level = 3`, `panic = "abort"`) para tiempos; `profiling` (= release + símbolos) para flamegraphs |

**Ruido**: la máquina es de uso diario y estaba cargada durante las mediciones (load average
31–52 en 12 hilos, navegador usando ~4–5 núcleos, 6,8 GiB de swap ocupado). Las cifras absolutas
tienen una variación grande (p. ej. escaneo frío 1 GB: 87–110 s entre ejecuciones); las
proporciones de los perfiles son estables. Para comparar fases, repetir en condiciones parecidas
o en una máquina en reposo.

## Datos sintéticos

`scripts/gen-fixtures.py <tamaño> [--seed 42]` → `src-tauri/target/perf-fixtures/<tamaño>/`
(ignorado por git). Reparto: Claude Code 60 %, Codex 15 %, Copilot 10 %, Cursor 10 %, Gemini 5 %;
20 proyectos; ventana de 90 días que termina el 2026-09-01 UTC. Desde `50m` incluye una sesión
de Claude Code de ~5 MiB (`sesion-grande-*.jsonl`).

| Tamaño | Bytes (`du -sb`) | Desviación | Archivos | Líneas | Registros | Tiempo de generación |
|---|---|---|---|---|---|---|
| `50m` | 52 429 259 | +0,001 % | 397 | 49 000 | 107 944 | 6,7 s |
| `200m` | 209 716 090 | +0,000 4 % | 1 674 | 195 563 | 430 688 | 23,5 s |
| `1g` | 1 073 743 094 | +0,000 1 % | 8 760 | 999 784 | 2 200 044 | 131,8 s |

## Reproducción

Herramientas de Rust dentro de distrobox
(`distrobox enter agentburn-dev -- bash -lc 'export PATH=$HOME/.cargo/bin:$PATH; cd src-tauri && …'`).
Comandos desde `src-tauri/` salvo el generador. Sin `AGENTBOARD_LOG` (trazas en `warn`).

```bash
# 1. Fixtures (desde la raíz del repo)
python3 scripts/gen-fixtures.py 50m
python3 scripts/gen-fixtures.py 200m
python3 scripts/gen-fixtures.py 1g
du -sb src-tauri/target/perf-fixtures/*

# 2. Escaneo frío + refresco en vivo + consultas (por tamaño)
cargo build --release --example perf_scan
./target/release/examples/perf_scan --fixtures target/perf-fixtures/50m --repeat 20
./target/release/examples/perf_scan --fixtures target/perf-fixtures/200m --repeat 20
./target/release/examples/perf_scan --fixtures target/perf-fixtures/1g --repeat 20

# 3. Consultas sin la contaminación del refresco en vivo (las cifras de la tabla de consultas)
./target/release/examples/perf_scan --fixtures target/perf-fixtures/50m --phase queries
./target/release/examples/perf_scan --fixtures target/perf-fixtures/200m --phase queries
./target/release/examples/perf_scan --fixtures target/perf-fixtures/1g --phase queries

# 4. Benchmarks criterion (necesitan el fixture 50m; si falta lo generan o lo indican)
cargo bench --bench parse_line --bench ingest_file --bench summary_queries

# 5. Flamegraphs con punteros de marco (ver «Flamegraphs»)
export CARGO_TARGET_DIR=target/fp RUSTFLAGS="-C force-frame-pointers=yes" CFLAGS="-fno-omit-frame-pointer"
cargo build --profile profiling --example perf_scan
perf record -F 499 --call-graph fp -o target/perf-cold.data -- \
  target/fp/profiling/examples/perf_scan --fixtures target/perf-fixtures/1g --phase cold
perf record -F 499 --call-graph fp -o target/perf-live.data -- \
  target/fp/profiling/examples/perf_scan --fixtures target/perf-fixtures/200m --phase live --repeat 200
flamegraph --perfdata target/perf-cold.data -o ../docs/perf/cold-scan.svg
flamegraph --perfdata target/perf-live.data -o ../docs/perf/live-refresh.svg
```

`perf_scan` solo lee la carpeta indicada (cinco proveedores JSONL con `with_roots`, nunca
`providers::all()`); el archivo de actividad simulada `claude_code/-home-dev-Proyectos-proyecto-00/perf-live-<pid>.jsonl`
se borra al terminar (comprobado con `find target/perf-fixtures -name 'perf-live-*'`: vacío).

## Resultados de `perf_scan` (release)

### Escaneo frío, refresco en vivo y memoria

| Fixture | Escaneo frío | Refresco en vivo, 1000 líneas (mediana de 20; min–max) | RSS pico (`VmHWM`) |
|---|---|---|---|
| 50 MB | **3,23 s** (3,05–3,81 s en otras ejecuciones) | **126 ms** (100–204 ms) | 35,6 MiB (21,5 MiB sin fase en vivo) |
| 200 MB | **15,5 s** (15,5–16,9 s) | **164 ms** (130–195 ms) | 72,6 MiB (60,7 MiB) |
| 1 GB | **86,9 s** (86,9–110,5 s) | **264 ms** (214–315 ms) | 275,5 MiB (264,6 MiB) |

- 200 refrescos seguidos sobre 200 MB (perfil `profiling`, bajo `perf`): mediana **515 ms**,
  de 170 ms a 1 046 ms: cada refresco es más lento a medida que crece la sesión que se escribe.
- El escaneo frío crece más que linealmente: ×4,8 de 50 a 200 MB y ×27 de 50 MB a 1 GB (×20 de datos).

### Consultas del Resumen (`--phase queries`, filtro fijo de los 30 últimos días de la ventana)

«Primera» = primera ejecución tras el escaneo (en frío); «mediana» = mediana de 5 repeticiones.

| Consulta | 50 MB primera / mediana | 200 MB primera / mediana | 1 GB primera / mediana |
|---|---|---|---|
| `summary` | 8,1 / 8,2 ms | 61,5 / 59,5 ms | 312,9 / 343,0 ms |
| `breakdown(project)` | 29,4 / 30,4 ms | 132,1 / 138,9 ms | 973,6 / 1 074,6 ms |
| `breakdown(model)` | 28,3 / 32,7 ms | 142,8 / 141,5 ms | 1 000,5 / 946,9 ms |
| `timeseries(day)` | 7,4 / 7,3 ms | 37,3 / 38,4 ms | 273,8 / 284,7 ms |
| `activity` | 12,1 / 11,4 ms | 65,7 / 58,7 ms | 494,9 / 483,2 ms |
| **Resumen completo** (18 llamadas de `useDashboardData`) | **169,6 / 148,2 ms** | **1 012 / 1 244 ms** | **6 740 / 6 798 ms** |

Con la fase en vivo antes (20 000 llamadas nuevas dentro del filtro) las consultas son 2–10× más
lentas (p. ej. `summary` 50 MB: 79,7 ms; Resumen completo 1 GB: 5,6–7,4 s); ver
`perf_scan --repeat 20` sin `--phase`.

### Frente a los presupuestos del proyecto

| Presupuesto | Medido | Estado |
|---|---|---|
| Escaneo frío 200 MB < 10 s | 15,5–16,9 s | **No cumple** (×1,6) |
| Resumen con ~200 MB indexados < 1,5 s | 1,0–1,2 s (solo backend, sin IPC ni render) | Cumple justo; 1 GB: 6,8 s (no cumple) |
| Consulta en frío < 300 ms | 200 MB: 37–143 ms ✔ · 1 GB: 274–1 001 ms (4 de 5 por encima) | Cumple en 200 MB; no en 1 GB |

## Criterion (`cargo bench`, fixture `50m`)

Dos ejecuciones: la primera (A) con la máquina menos cargada, al implementar cada bench; la
segunda (B), `cargo bench` completo con load average 31–52. La diferencia es ruido del entorno
(no hubo cambios en `agentboard_lib` entre ambas).

| Bench | A (mediana) | B (mediana) |
|---|---|---|
| `parse_line/claude_code` | 4,62 µs | 14,47 µs |
| `parse_line/codex` | 3,86 µs | 3,51 µs |
| `parse_line/copilot` | 1,39 µs | 1,70 µs |
| `parse_line/cursor` | 2,46 µs | 12,76 µs |
| `parse_line/gemini` | — | 34,80 µs |
| `ingest_file/claude_code_5mib` (5 MiB, base nueva) | 232,9 ms · 20,5 MiB/s | 1,704 s · 2,8 MiB/s |
| `summary_queries/summary` | 7,03 ms | 8,54 ms |
| `summary_queries/breakdown(project)` | 25,33 ms | 27,54 ms |
| `summary_queries/breakdown(model)` | 24,63 ms | 27,15 ms |
| `summary_queries/timeseries(day)` | 5,42 ms | 6,35 ms |
| `summary_queries/activity` | 9,31 ms | 11,03 ms |
| `summary_queries/resumen_completo` | 154,3 ms | 243,4 ms |

## Flamegraphs

- [`cold-scan.svg`](cold-scan.svg): escaneo frío del fixture 1 GB (`--phase cold`), 49 043 muestras.
- [`live-refresh.svg`](live-refresh.svg): fixture 200 MB, `--phase live --repeat 200` (1000 líneas por
  refresco). El escaneo frío previo queda en el perfil (~14 % de las muestras, rama
  `perf_scan::cold`); el resto es el refresco.

**Cómo se obtuvieron**: `perf` funciona en distrobox con `perf_event_paranoid = 2` (muestreo de
espacio de usuario del propio proceso). El primer intento, `cargo flamegraph --profile profiling`
(desenrollado DWARF), dio pilas rotas: el 70 % de las muestras aparecía en `malloc`/`free` sin
llamadores y perf perdió bloques (3,2 GB de `perf.data`). Se repitió con **punteros de marco**
(`-C force-frame-pointers=yes`, el glibc de Fedora ya los tiene) y `--call-graph fp`: pilas completas
y 9 MB de datos. Los comandos exactos están en «Reproducción».

## Riesgo de `panic = "abort"` (tarea 1.3)

Con `abort` un pánico en el hilo de escaneo o del vigilante cierra la app entera (antes solo
mataba ese hilo). Revisión de `unwrap`/`expect` fuera de tests:

- `ingest.rs`, `watcher.rs` y `providers/*`: ninguno.
- Alcanzables desde los hilos de fondo (tras cada escaneo, `alerts::refresh` y `tray::refresh`):
  `alerts.rs:91` (`and_hms_opt(0,0,0).expect`), `tz.rs:37,49` (`timestamp_millis_opt(..).unwrap()`)
  y `tz.rs:42,54` (`expect` de medianoche/hora en punto). No pueden fallar en la práctica
  (conversión desde UTC siempre única; 00:00 siempre válida), pero conviene sustituirlos por
  errores en una fase posterior.
- Hilo principal: `lib.rs:144` (`default_window_icon().unwrap()`) y `.expect("error al arrancar")`.
- Otros (consultas, no hilos de fondo): `sessions.rs:304,320`, `findings.rs:233,593`.

No se añadió ningún `unwrap()` en rutas de ingesta en esta fase.

## Top 5 cuellos de botella

Ordenados por impacto sobre los presupuestos. Los % son de muestras del perfil con punteros de
marco (inclusivos salvo que se diga «propio»).

### 1. Ingesta: cada registro recompila su SQL (`Connection::execute` sin caché de sentencias)

- **Evidencia**: escaneo frío 1 GB, `Writer::apply` = **83,0 %**; dentro, `sqlite3Prepare` =
  **57,2 %** (análisis sintáctico y generación de código de cada `INSERT … ON CONFLICT`, con
  `sqlite3GenerateConstraintChecks` 18,8 %); `sqlite3_step` (la inserción real) solo ~26 %. El
  parseo (`parse_line` de los cinco proveedores) suma el 8,7 % (Claude Code 5,0 %). `ingest_file` 5 MiB: 233 ms (20,5 MiB/s).
  Escaneo frío 200 MB 15,5 s frente a un presupuesto de 10 s.
- **Arreglo probable**: `prepare_cached` / sentencias preparadas una vez por transacción en un
  escritor único con lotes; de paso, menos `malloc` (7,6 % en `_int_malloc`).
- **Fase**: 2 (escritor único con `prepare_cached`), con el lote y el canal de escritura de la
  Fase 1.

### 2. Resumen: `breakdown` y la vista `call_costs` recorren toda la tabla `calls` sea cual sea el filtro

- **Evidencia**: perfil de `--phase queries` (200 MB): `breakdown` = **54 %** de las muestras de
  consultas (8 de las 18 llamadas del Resumen), con el tiempo en `sqlite3VdbeExec` y búsquedas de
  B-tree (`sqlite3BtreeIndexMoveto` 12,7 %, `sqlite3BtreeTableMoveto` 10,2 %). En el código:
  - el CTE `firsts` calcula `ROW_NUMBER() OVER (PARTITION BY session_id ORDER BY ts)` sobre
    **todas** las llamadas, sin aplicar el filtro;
  - `call_costs` hace por cada fila una subconsulta correlacionada `MAX(valid_from)` en `prices`.

  Con un filtro fijo de 30 días el coste crece con el total de la base: `breakdown(project)`
  29 → 132 → 974 ms y el Resumen completo 0,15 → 1,0–1,2 → 6,8 s (50 MB → 200 MB → 1 GB).
- **Arreglo probable**: guardar coste y «primera llamada de la sesión» al ingerir (o en tablas de
  resumen incrementales), filtrar antes de agregar, índices compuestos `(ts, session_id)`
  comprobados con `EXPLAIN QUERY PLAN`.
- **Fase**: 2 (agregados incrementales, índices, caché por `data_generation`).

### 3. Refresco en vivo: `backfill_turns` reescribe todas las llamadas sin turno de cada sesión tocada

- **Evidencia**: `live-refresh.svg`: `Writer::backfill_turns` = **58,3 %** del perfil total
  (~68 % del tiempo de refresco), frente a `Writer::apply` 22,2 %. Para cada sesión tocada lanza
  `UPDATE calls/tool_calls SET turn_id = (SELECT … ORDER BY t.ts DESC LIMIT 1) WHERE session_id = ?
  AND turn_id IS NULL`, que recorre todas las llamadas de la sesión (el índice es solo
  `session_id`). El refresco pasa de 170 ms a 1 046 ms a lo largo de 200 refrescos (mediana 515 ms)
  según crece la sesión. *Matiz*: las líneas simuladas no tienen prompt de usuario, así que todas
  quedan sin turno y es el peor caso; en sesiones reales afecta a las llamadas anteriores al
  primer turno y a las de subagentes, y el recorrido por `session_id` ocurre igualmente.
- **Arreglo probable**: rellenar el turno solo de las filas insertadas en esa pasada (o asignarlo
  al insertar con el estado del proveedor) y/o índice parcial `(session_id, ts) WHERE turn_id IS NULL`.
- **Fase**: 1 (ingesta incremental); el índice, en la 2.

### 4. El Resumen son 18 consultas secuenciales tras un único `Mutex`

- **Evidencia**: la llamada completa del Resumen cuesta lo mismo que la suma de sus 18 consultas
  (200 MB: 1,0–1,2 s; 1 GB: 6,8 s), cada una vuelve a recorrer `call_costs`. Los comandos y el
  vigilante comparten `Arc<Mutex<Connection>>`: mientras un refresco escribe (126–515 ms en las
  mediciones) ninguna consulta avanza, y viceversa. Spans `cmd.*` en la app (`AGENTBOARD_LOG=debug`)
  muestran las llamadas una tras otra.
- **Arreglo probable**: comando único `get_dashboard_snapshot(filter)`, conexiones de solo lectura
  separadas del escritor y caché de resultados invalidada por generación de datos.
- **Fase**: 3 (`get_dashboard_snapshot`, eventos sin datos), con lectores separados y caché de la Fase 2.

### 5. Cada refresco en vivo vuelve a escanear todos los archivos de todas las raíces

- **Evidencia**: el vigilante llama a `scan_all` completo: `register_agents`, `DELETE` de proyectos
  huérfanos, `walk` + ordenación de todas las rutas (2,1 % del perfil en vivo, 1,4 % solo en
  ordenar `PathBuf`), `stat` y un `SELECT` en `file_state` por archivo (2,7 %). Con la misma
  sesión creciendo igual (20 × 1000 líneas), el refresco cuesta 126 ms con 397 archivos,
  164 ms con 1 674 y 264 ms con 8 760 (≈16 µs por archivo, que no cambian).
- **Arreglo probable**: releer solo las rutas que notifica `notify` (debounce por archivo, filtrar
  eventos `Access` y rutas que no casan con el proveedor), sin `walk` global.
- **Fase**: 3 (vigilante).

### Lo que no es cuello de botella (de momento)

- Parseo JSON: 1,4–4,6 µs por línea (criterion A); 8,7 % del escaneo frío (los cinco proveedores).
- Memoria: 265–276 MiB de RSS pico con 1 GB de logs (2,2 M registros); 22–36 MiB con 50 MB.
- Recorrido de carpetas en frío (`walk`): 0,02 % del perfil.
- Comprobaciones de claves foráneas al insertar: `sqlite3FkCheck` 7,7 % + `fkScanChildren` 6,5 %
  en frío; candidatas a revisar junto al punto 1 (pueden explicar parte del crecimiento
  más que lineal del escaneo frío, junto con la profundidad de los B-tree).
