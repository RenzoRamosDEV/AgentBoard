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

# 3b. Fase 2: huellas de datos y de consultas, caché en disco, arranque con caché y contención
./target/release/examples/perf_scan --fixtures target/perf-fixtures/200m --phase cold --fingerprint --query-fingerprint
./target/release/examples/perf_scan --fixtures target/perf-fixtures/200m --phase cold --cache target/perf-cache/200m.sqlite
./target/release/examples/perf_scan --fixtures target/perf-fixtures/200m --phase warm --cache target/perf-cache/200m.sqlite
./target/release/examples/perf_scan --fixtures target/perf-fixtures/200m --phase contention --repeat 200 --cache target/perf-cache/cont.sqlite
rm -f target/perf-cache/*.sqlite*

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

## Después de la Fase 1 (ingesta y parseo)

Cambio OpenSpec `perf-fase-1-ingesta`. Mismo portátil y contenedor que la Fase 0 (Ryzen 5 Pro
7535U, 14 GiB, Bazzite kernel 7.2.9, distrobox `agentburn-dev`, rustc 1.98.1), perfil `release`,
binario de `1394e5a` (rama `perf/rendimiento-fluido`). Mismos fixtures (`50m`, `200m`, `1g`, semilla 42).

**Carga de la máquina**: muy variable durante la fase por otro trabajo ajeno en paralelo (load
average entre 4 y 125). Las cifras de la tabla son de una tanda con **load average 5–7** (más
tranquila que la de la Fase 0, 31–52). La misma tanda completa repetida con load 15–108 dio frío
200 MB de 2,8 a 9,7 s, refresco con `--repeat 200` de 157 a 239 ms y Resumen 1 GB de 8,5 a 22,7 s:
con la máquina saturada el presupuesto de 200 ms del refresco **no** se cumple. Las comparaciones
entre pasos de la fase se hicieron A/B alternando binarios (ver `design.md` del cambio).

Huella de resultados de `200m` (`perf_scan --fingerprint`) idéntica a la grabada antes de
optimizar (tarea 1.6); los golden de `tests/golden/` (fixture `2m` y `tests/fixtures`) siguen
iguales.

### Antes y después

| Métrica | Fase 0 | Partida de la Fase 1 (`5bb39eb`) | Después de la Fase 1 | Cambio frente a la Fase 0 |
|---|---|---|---|---|
| Escaneo frío 50 MB | 3,23 s (3,05–3,81) | — | **0,59 s** (0,59–0,59) | −82 % |
| Escaneo frío 200 MB | 15,5 s (15,5–16,9) | 17,6 s (17,0–17,8) | **2,73 s** (2,68–2,78) | −82 % |
| Escaneo frío 1 GB | 86,9 s (86,9–110,5) | — | **17,9 s** (16,9–23,1) | −79 % |
| Refresco 1000 líneas, mediana de 20 — 50 MB | 126 ms | — | **25,6 ms** | −80 % |
| Refresco 1000 líneas, mediana de 20 — 200 MB | 164 ms | — | **55,1 ms** | −66 % |
| Refresco 1000 líneas, mediana de 20 — 1 GB | 264 ms | — | **155,8 ms** | −41 % |
| 200 refrescos sobre 200 MB: mediana | 515 ms (`profiling` bajo `perf`) | 548 ms (546–649) | **48,2 ms** (46,1–48,2) | −91 % |
| 200 refrescos: primeros 20 / últimos 20 | 170 → 1 046 ms (min–max) | 184–242 / 818–874 ms | **44–50 / 49–52 ms** (×1,0–1,1) | ya no crece |
| RSS pico solo frío 50 / 200 MB / 1 GB | 21,5 / 60,7 / 264,6 MiB | — / 56 (paso 2) / — | 36,8 / 83 / 290 MiB | +71 / +37 / +10 % |
| RSS pico con `--repeat 20` 50 / 200 MB / 1 GB | 35,6 / 72,6 / 275,5 MiB | — | 52,0 / 97,4 / 306,3 MiB | +46 / +34 / +11 % |

Consultas del Resumen (`--phase queries`, primera / mediana de 5). El código de consultas no
cambia en esta fase; las diferencias son ruido y el algo más de datos por la caché de sentencias.

| Consulta | 50 MB | 200 MB | 1 GB |
|---|---|---|---|
| `summary` | 11,6 / 10,7 ms (F0: 8,1 / 8,2) | 65,1 / 65,2 ms (F0: 61,5 / 59,5) | 499,5 / 394,2 ms (F0: 312,9 / 343,0) |
| `breakdown(project)` | 37,2 / 32,9 ms (F0: 29,4 / 30,4) | 182,2 / 171,0 ms (F0: 132,1 / 138,9) | 928,7 / 947,5 ms (F0: 973,6 / 1 074,6) |
| `breakdown(model)` | 29,6 / 28,5 ms (F0: 28,3 / 32,7) | 171,7 / 164,8 ms (F0: 142,8 / 141,5) | 917,4 / 885,3 ms (F0: 1 000,5 / 946,9) |
| `timeseries(day)` | 7,9 / 7,5 ms (F0: 7,4 / 7,3) | 53,4 / 49,7 ms (F0: 37,3 / 38,4) | 273,1 / 274,5 ms (F0: 273,8 / 284,7) |
| `activity` | 11,9 / 13,0 ms (F0: 12,1 / 11,4) | 72,3 / 78,0 ms (F0: 65,7 / 58,7) | 450,9 / 448,9 ms (F0: 494,9 / 483,2) |
| **Resumen completo** (18 llamadas) | 190,2 / 191,6 ms (F0: 169,6 / 148,2) | 1 119 / 1 152 ms (F0: 1 012 / 1 244) | 6 164 / 7 842 ms (F0: 6 740 / 6 798) |

Criterion (fixture `50m`; dos tandas: B con load 6–10 al final de la fase, C con load ~100;
`parse_file/<agente>` es nuevo en esta fase: parseo del archivo más grande de cada proveedor):

| Bench | Fase 0 (A / B) | Fase 1 (B / C) |
|---|---|---|
| `parse_line/claude_code` | 4,62 / 14,47 µs | 6,31 / 3,42 µs |
| `parse_line/codex` | 3,86 / 3,51 µs | 4,65 / 2,81 µs |
| `parse_line/copilot` | 1,39 / 1,70 µs | 2,99 / 1,82 µs |
| `parse_line/cursor` | 2,46 / 12,76 µs | 3,92 / 2,72 µs |
| `parse_line/gemini` | — / 34,80 µs | 6,66 / 7,94 µs |
| `parse_file/claude_code` | — | 13,8 ms (346 MiB/s) / 17,0 ms |
| `parse_file/codex` · `copilot` · `cursor` · `gemini` | — | 2,02 · 1,69 · 1,76 · 2,94 ms / 1,94 · 1,48 · 1,03 · 2,24 ms |
| `ingest_file/claude_code_5mib` | 232,9 ms (20,5 MiB/s) / 1,704 s | 91,0 ms (52,5 MiB/s) / 51,2 ms (93,2 MiB/s) |
| `summary_queries/resumen_completo` | 154,3 / 243,4 ms | 319,1 / 189,7 ms |

Las comparaciones fiables de los benches de parseo son las A/B de cada paso (mismo momento,
binarios alternos): `parse_line` −13 % a −50 % y `parse_file` −18 % a −40 % según el proveedor.

### Frente a los presupuestos

| Presupuesto | Medido | Estado |
|---|---|---|
| Escaneo frío 200 MB < 10 s | 2,68–2,78 s (load 6); 2,8–9,7 s con la máquina saturada | **Cumple** |
| Mediana de 200 refrescos de 1000 líneas < 200 ms | 46–48 ms (load 6); 157–239 ms con load 35–40 | **Cumple** en condiciones normales |
| Últimos 20 refrescos ≤ 1,5 × primeros 20 | ×0,97–1,10 | **Cumple** |
| Resumen con ~200 MB < 1,5 s (Fase 0) | 1,12–1,15 s (solo backend) | Cumple justo, sin cambios (Fase 2) |
| Consulta en frío < 300 ms (Fase 0) | 200 MB: 50–182 ms ✔ · 1 GB: 273–929 ms | Sin cambios (Fase 2) |

### Pasos conservados y revertidos

Todos los pasos se conservaron (regla D9: mejora ≥ 5 % de su métrica o paso estructural que no
empeora > 5 %); ninguno se revirtió:

1. Sentencias en caché (`prepare_cached`, caché de 64): frío 200 MB 17,6 → 4,8 s.
2. Relleno de turnos acotado a los turnos nuevos: refresco 548 → 50 ms y deja de crecer.
3. Estado de archivos cargado una vez + `BufReader::read_until`: frío 4,2 → 3,5 s; vivo −9 %.
4. Lectura y parseo en paralelo con escritura en orden: frío −12 % (A/B); la primera versión
   empeoraba el refresco un 23 % y se ajustó (los archivos sin cambios se descartan al planificar).
5. Hilo de ingesta dedicado y comandos `async` (estructurales): las consultas de la interfaz se
   completan durante el escaneo inicial (116–172 comandos en la app, mediana 96–165 ms).
6. Parseo mínimo con sonda por proveedor: `parse_line` −13 % a −50 %; frío −3 % a −6 % (lo marca
   la escritura).
7. Un único `UPSERT` por sesión y archivo (opcional D2b): frío −29 %, vivo −11 % (A/B).

### Flamegraph

- [`cold-scan-fase1.svg`](cold-scan-fase1.svg): escaneo frío del fixture 1 GB (`--phase cold`),
  mismos comandos que en «Reproducción» con `-o target/perf-cold-fase1.data` y
  `-o ../docs/perf/cold-scan-fase1.svg`.
  20 374 muestras, todos los hilos (perfil `profiling` compilado con `CARGO_BUILD_JOBS=2` por falta de
  memoria con la máquina cargada). `sqlite3Prepare` pasa del 57,2 % al **0,08 %**; el escritor
  (`write_in_order` → `write_file`) es el 71 % de las muestras, casi todo `sqlite3_step` (35,8 %;
  `sqlite3BtreeInsert` 17,5 %); los lectores en paralelo (`read_jobs` → `read_file`) suman el
  28,5 % repartido entre núcleos (`parse_line` de Claude Code 9,7 %); `backfill_turns` 4,0 %.

### Seguimientos para la Fase 2

- Comprobaciones de claves foráneas al insertar (`sqlite3FkCheck` + `fkScanChildren`, ~14 % del
  frío en la Fase 0): ahora que la compilación de SQL desapareció pesan más.
- Índice parcial `(session_id, ts) WHERE turn_id IS NULL`: el relleno aún recorre la sesión cuando
  llega un turno nuevo.
- Memoria: el pipeline paralelo mantiene hasta 4 × núcleos archivos parseados en vuelo; RSS pico
  +10 % (1 GB) a +71 % (50 MB). Si importa, bajar la ventana o el tamaño de lote.
- Contención del `Mutex` único entre consultas e ingesta (cuello #4): con 200 MB la interfaz
  encadena tandas de 18 consultas y una relectura del vigilante puede esperar decenas de segundos.
- Consultas del Resumen y `call_costs` (cuello #2): sin cambios en esta fase.

## Después de la Fase 2 (SQLite y caché en disco)

Cambio OpenSpec `perf-fase-2-sqlite`. Mismo portátil, contenedor y fixtures que las fases
anteriores. Binarios `release`: partida `d7fa167` (golden de consultas grabado, sin
optimizaciones) frente a final `231b00e`, A/B alternando binarios, tres pares por métrica, con
`TZ=Europe/Madrid` (lo fija `perf_scan`: la zona de la caché es la de la interfaz, como en la app).

**Carga de la máquina**: tanda final con load average 1,5–3,5, salvo el frío de 1 GB, que coincidió
con un pico (6–12). Para ese caso se da la tanda anterior, con load 1,4–2,3 y el mismo código de
ingesta salvo el `stat` único.

**Equivalencia**:
- Golden de ingesta (`tests/golden/ingesta_*.txt`) y de consultas (`tests/golden/consultas_*.txt`,
  grabados en la partida, en memoria y en disco): iguales, sin regrabar.
- Huella de tablas de `200m` (`--fingerprint`): idéntica.
- Huella de consultas (`--query-fingerprint`): 36 de 200 líneas cambian de hash, todas de
  `timeseries_by`, porque el hash redondea cada real a 6 decimales y el orden de las sumas lo
  mueve en el borde. Las 200 salidas completas de `200m`, comparadas con el comparador del golden
  (enteros exactos, reales ≤ 1e-6), son **iguales**.

### Antes y después

| Métrica | Partida (`d7fa167`) | Después de la Fase 2 (`231b00e`) | Cambio |
|---|---|---|---|
| Resumen completo 200 MB, en frío | 1 076 ms (1 024–1 087) | **171 ms** (169–175) | −84 % |
| Resumen completo 200 MB, repetido | 1 066 ms | **0,0 ms** (caché de resultados) | — |
| Resumen completo 1 GB, en frío | 5 692 ms (5 657–5 799) | **959 ms** (780–985) | −83 % |
| Resumen completo 1 GB, repetido | 5 684 ms | **0,0 ms** | — |
| `summary` · `breakdown(project)` · `breakdown(model)` · `timeseries(day)` 200 MB | 65,7 · 173,5 · 159,7 · 49,6 ms | 0,9 · 0,9 · 0,7 · 0,5 ms | −98 a −99 % |
| Los mismos, 1 GB | 352 · 844 · 814 · 256 ms | 3,8 · 3,9 · 3,4 · 1,8 ms | −99 % |
| `activity` 200 MB / 1 GB | 73,4 / 420 ms | 47,7 / 249 ms | −35 / −41 % |
| Consulta más lenta del Resumen 1 GB | 844 ms (`breakdown(project)`) | 341 ms (`activity_daily`, detalle) | −60 % |
| Arranque con la caché llena hasta datos del Resumen 50 MB / 200 MB / 1 GB | 0,4 / 1,8 / 11,5 s (había que releer todo) | **25 / 140 / 795 ms** | −94 a −93 % |
| Escaneo incremental sin cambios al arrancar 50 MB / 200 MB / 1 GB | — | 6 / 20 / 98 ms | — |
| Frío en memoria 50 MB / 200 MB | 404 / 1 833 ms | 517 / 2 368 ms | +28 / +29 % |
| Frío en memoria 1 GB (tanda con load 2) | 11,5 s | 14,1 s | +22 % |
| Frío a la caché en disco 50 MB / 200 MB / 1 GB | — | 584 / 2 783 / 20 003 ms | — |
| Refresco 1000 líneas, `--repeat 20`, 50 MB / 200 MB / 1 GB | 17,6 / 31,8 / 111,0 ms | 19,9 / 35,3 / 120,3 ms | +13 / +11 / +8 % |
| 200 refrescos sobre 200 MB: mediana | 33,7 ms | 37,9 ms | +12 % |
| 200 refrescos: primeros 20 / últimos 20 | 34,8 / 34,8 ms | 34,8 / 39,7 ms (×1,14) | — |
| Refresco con el Resumen en bucle por los lectores (caché en disco) | — (`Mutex` único: esperaba decenas de s) | ×1,36–1,38 (42 → 58 ms) | — |
| RSS pico: frío 200 MB · consultas 1 GB · vivo 1 GB | 81 · 297 · 299 MiB | 88 · 320 · 323 MiB | +9 / +8 / +8 % |
| Tamaño de `cache.sqlite` 50 MB / 200 MB / 1 GB | — | 10,1 / 39,9 / 204,6 MiB | — |

Criterion (fixture `50m`, `cargo bench`, load ~2). La caché de la base de medición está en la
zona de las consultas, así que el Resumen lee de los agregados:

| Bench | Fase 1 (B / C) | Fase 2 |
|---|---|---|
| `summary_queries/resumen_completo` | 319,1 / 189,7 ms | **19,3 ms** |
| `summary_queries/summary` · `breakdown(project)` · `breakdown(model)` · `timeseries(day)` | — | 108 · 158 · 126 · 86 µs |
| `summary_queries/activity` | — | 5,41 ms |
| `ingest_file/claude_code_5mib` | 91,0 / 51,2 ms | 41,9 ms |
| `parse_file/claude_code` · `codex` · `copilot` · `cursor` · `gemini` | 13,8 · 2,02 · 1,69 · 1,76 · 2,94 ms / 17,0 · 1,94 · 1,48 · 1,03 · 2,24 ms | 9,63 · 1,25 · 1,06 · 0,73 · 1,24 ms |
| `parse_line/claude_code` · `codex` · `copilot` · `cursor` · `gemini` | 6,31 · 4,65 · 2,99 · 3,92 · 6,66 µs / 3,42 · 2,81 · 1,82 · 2,72 · 7,94 µs | 2,41 · 2,16 · 1,29 · 1,56 · 3,83 µs |

El parseo no cambia en esta fase: sus diferencias con la Fase 1 son de carga de la máquina.

### Frente a los presupuestos

| Presupuesto (spec `rendimiento`) | Medido | Estado |
|---|---|---|
| Arranque con 200 MB indexados < 1,5 s | 138–144 ms | **Cumple** |
| Resumen 200 MB < 300 ms en frío | 169–175 ms | **Cumple** |
| Resumen 200 MB < 100 ms desde la caché de resultados | 0,0 ms | **Cumple** |
| Cada consulta del Resumen sobre 1 GB < 300 ms | 17 de 18; `activity_daily` 341 ms (312–391) | **No cumple** (por poco, una consulta de detalle) |
| Frío 200 MB a la caché en disco < 10 s | 2,78–2,85 s | **Cumple** |
| Refresco con lecturas ≤ 1,1 × sin ellas | ×1,36–1,38 en disco (×9 en «Solo memoria», espera acotada admitida) | **No cumple** |
| (Fase 1) mediana de 200 refrescos < 200 ms; últimos 20 ≤ 1,5 × primeros 20 | 37,9 ms; ×1,14 | Cumple |

### Pasos conservados y revertidos

Todos se conservaron salvo el 7, aunque varios cambiaron tras medir (detalle y cifras por paso en
`design.md` del cambio, «Desviaciones durante la implementación»):

1. Golden de consultas y huella antes de optimizar.
2. Coste precalculado: con disparadores, el frío empeoraba +123 %. Cualquier disparador en `calls`
   o un `INSERT … SELECT` abre un subdiario por sentencia. Se conservó sin disparadores de
   inserción: el coste lo calcula el escritor (`PriceBook`). Consultas −20 a −50 %.
3. Primera llamada de la sesión sin `ROW_NUMBER`: `breakdown` −64 %.
4. Día local precalculado (estructural, +1 % en frío).
5. Agregados diarios. Correcciones por medición: `CROSS JOIN` para no recorrer `calls` en cada
   lote; deltas para las inserciones (el refresco en vivo crecía con la sesión).
6. Índices revisados con `EXPLAIN QUERY PLAN`: fuera `calls_agent`; ninguno nuevo.
7. **Revertido**: claves foráneas desactivadas. Sin mejora (razón 0,99 en 12 pares) una vez que
   el escritor inserta sin disparadores; vuelve `foreign_keys=ON`.
8. Escritor único y lectores (estructural).
9. Caché de resultados.
10. Caché en disco. Correcciones: `mmap` y `journal_size_limit`; `TZ` manda sobre la zona
    detectada.
11. Caché fiel a los logs. Corrección: un solo `stat` por archivo para la limpieza y el plan.
12. Ajustes «Solo memoria» y «Reconstruir caché».

### Seguimientos para la Fase 3

- **Frío +22–29 % y vivo +8–13 %**: mantener coste, día, primera llamada, agregados y
  `session_files` cuesta. Candidatos: aplazar el refresco de derivados al final del escaneo
  inicial en lugar de cada lote, y memorizar el día local por rango en el escritor.
- **Contención en disco (×1,37)**: con un lector siempre activo, el checkpoint del WAL no puede
  reiniciarlo, el WAL crece y las lecturas se vuelven más lentas (el Resumen pasa de 140 ms a
  2,3 s tras 200 refrescos). Opciones: checkpoint `TRUNCATE` cuando la interfaz no lee, o las
  relecturas dirigidas del vigilante (menos lotes).
- **`activity_daily` / `activity` / `timeseries(hour)` en 1 GB** (341 / 249 / 149 ms): siguen por
  el detalle (turnos, horas). Pueden pasar a agregados por turno o por hora.
- **Huella de consultas**: el hash redondeado no sirve para comparar fases cuando cambia el orden
  de las sumas. Mejor volcar las salidas y compararlas con tolerancia (como el golden).
