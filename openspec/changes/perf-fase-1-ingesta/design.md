# Design

## Context

- Motivación: ver proposal.md (Why). Requisitos: ver `specs/rendimiento/spec.md`. Cifras de partida y ranking: `docs/perf/BASELINE.md` (commit medido `cd5d4fa`).
- Flujo actual observado:
  - `lib.rs::run`: hilo de escaneo inicial que toma `Arc<Mutex<Connection>>` y llama a `ingest::scan_all(&mut conn, &providers::all())` con el `Mutex` cogido durante todo el escaneo; luego emite `ingest://done` (con `ScanStats`) y refresca bandeja/avisos.
  - `watcher::run`: tras el debounce llama a `scan_all` con sus propias instancias de proveedor (`readers`), también con el `Mutex` cogido todo el escaneo; si hubo registros, `on_change` emite `ingest://done` (sin datos) y refresca avisos. El escaneo inicial y el vigilante usan instancias distintas, así que el primer refresco del vigilante relee desde 0 los archivos de proveedores con estado (`knows() == false`).
  - `scan_all`: `register_agents` → `DELETE` de proyectos huérfanos → por proveedor/raíz: `walk` (ordenado) → `ingest_file` secuencial. `ingest_file`: `SELECT` de `file_state` por archivo, `read_to_end` desde el offset, `parse_line` por línea dentro de la transacción, `Writer::apply` por registro con `Transaction::execute` (compila el SQL cada vez), `backfill_turns` al final y `UPSERT` de `file_state`.
  - `Writer::backfill_turns`: por cada sesión tocada, `UPDATE calls/tool_calls … WHERE session_id = ? AND turn_id IS NULL`, aunque no haya llegado ningún turno.
  - El turno de una llamada se fija al insertarla (`SELECT id FROM turns … ts <= ? ORDER BY ts DESC LIMIT 1`) y no se sobrescribe (`COALESCE`): **el resultado depende del orden en que se aplican los archivos**.
  - Proveedores con estado entre líneas (Codex, Copilot, Cursor, Gemini) y Claude Code (`first_cwd`) guardan su estado por ruta en `Mutex<HashMap<PathBuf, _>>`; son `Send + Sync`. Todos parsean con `serde_json::Value`. Claude Code emite un `Record::Session` por cada línea con `sessionId` y `timestamp`, sea del tipo que sea (afecta a `started_at`/`ended_at`).
  - Deduplicación ya resuelta por clave: `calls.message_id` PK, `tool_calls.call_id` UNIQUE, `turns.id` PK, `events UNIQUE(session_id, ts, kind)`, `projects.cwd` UNIQUE; todo con `ON CONFLICT`/`INSERT OR IGNORE`, sin `SELECT` previo (salvo `project_id`, cacheado por transacción).
  - `commands.rs`: comandos síncronos; `with_db` bloquea el `Mutex` en el hilo que Tauri use para el comando; `get_settings`, `list_prices` y `set_settings` leen/escriben el archivo de ajustes.
  - `expect`/`unwrap` alcanzables desde hilos de fondo: `alerts.rs:91`, `tz.rs:37,42,49,54` (perfil release con `panic = "abort"`).
  - El servidor MCP (`mcp/src/main.rs:60`) y `perf_scan` llaman a `ingest::scan_all(&mut conn, providers)`.
- Herramientas de la Fase 0 reutilizadas: `scripts/gen-fixtures.py`, `examples/perf_scan.rs`, `benches/{parse_line,ingest_file,summary_queries}.rs`, `benches/common/mod.rs` (compartido vía `#[path]` y probado en `tests/perf_common.rs`), `telemetry.rs`.

## Goals / Non-Goals

**Goals:**
- Atacar los cuellos #1 (recompilación de SQL) y #3 (`backfill_turns`) de la línea base y cumplir los presupuestos de ingesta del spec.
- Sacar la ingesta del camino de la interfaz sin cambiar la API de comandos ni los eventos.
- Equivalencia exacta de resultados, verificada con huella sobre fixtures sintéticos en cada paso.
- Cero dependencias nuevas.

**Non-Goals:**
- Conexiones de lectura separadas, base en disco/compartida, tablas de resumen, caché de resultados, índices nuevos (incluido el parcial `(session_id, ts) WHERE turn_id IS NULL`), cambios de esquema.
- Relecturas dirigidas por ruta, cambios de debounce o eventos `data:changed` del vigilante (el vigilante sigue pidiendo un escaneo completo).
- Revisar las comprobaciones de claves foráneas (`sqlite3FkCheck` ~14 % del frío): fuera de esta fase; se anota como seguimiento.
- Frontend.

## Decisions

### D1. Huella de resultados y golden antes de tocar nada
- Función `fingerprint(conn) -> String` en `benches/common/mod.rs` (compartida con `perf_scan` y los tests): texto canónico con, por agente, nº de sesiones, llamadas, herramientas, turnos y eventos, suma de cada columna de tokens y coste total (vista `call_costs`, redondeado a 1e-9); y por tabla un hash FNV-1a 64 (implementado a mano, ~10 líneas, estable entre versiones de Rust) de sus filas ordenadas por **clave natural**, excluyendo ids autoincrementales y marcas de reloj (`first_seen`, `last_scan`): `calls` por `message_id` (incluye `turn_id`), `tool_calls` por `call_id`, `turns` por `id`, `sessions` por `id` (proyecto como `cwd`), `projects` por `cwd`, `events` por `(session_id, ts, kind)`.
- Test `src-tauri/tests/equivalencia_ingesta.rs`: genera el fixture `2m` (semilla fija) con `gen-fixtures.py` como `fixtures_sinteticos.rs` (se salta si no hay `python3`), escanea con los cinco proveedores JSONL y compara con `src-tauri/tests/golden/ingesta_2m.txt`; además escanea `tests/fixtures` (incluye OpenCode) contra `golden/ingesta_fixtures.txt`. Con `UPDATE_GOLDEN=1` reescribe el golden. Los golden se graban **en el commit de partida, antes de cualquier optimización**, y no se vuelven a grabar en esta fase.
- Equivalencia a escala: `perf_scan --fingerprint` imprime la huella tras el escaneo frío; se guarda la del fixture `200m` en la tarea de partida y se compara al final.
- **Alternativas**: comparar solo `ScanStats` + `summary` (no detecta cambios de turnos ni de filas concretas); volcar todas las filas al golden (archivo de MB, difícil de revisar); `DefaultHasher` (no garantiza estabilidad entre versiones).

### D2. Escritor con sentencias en caché
- `Writer` usa `tx.prepare_cached(SQL)` + `execute(params)` para todas sus sentencias (sesión, llamada, modelo de sesión, tool use + su `UPDATE` de relectura, tool result, turno, evento, proyecto) y para `file_state`. SQL constantes (`const`), sin `format!` en la ruta caliente (el `backfill` usa dos constantes, una por tabla).
- Capacidad de la caché de sentencias de la conexión ≥ 32 (`set_prepared_statement_cache_capacity` en `db::open_in_memory`; rusqlite trae 16 y hay ~14 sentencias distintas en ingesta, más las de consultas).
- Paso opcional medido (D2b): coalescer en memoria los `Record::Session` repetidos de un mismo archivo (Claude Code emite uno por línea) — primera aparición se inserta (las llamadas la referencian por clave foránea), las siguientes acumulan `min(ts)`, `max(ts)`, última rama no nula, primer proyecto no nulo y `max(is_subagent)`, y se vuelcan con un único `UPSERT` al final del archivo; igual para `UPDATE sessions SET model` (último modelo no sidechain). Solo se conserva si mejora (regla de D9) y la huella no cambia.
- **Por qué**: `sqlite3Prepare` es el 57 % del escaneo frío; con caché cada sentencia se compila una vez por conexión.
- **Alternativas**: `INSERT` multi-fila (más complejo con `ON CONFLICT … DO UPDATE` y límites de parámetros; se puede reconsiderar si D2 no basta); `PRAGMA`s de durabilidad (base en memoria: sin efecto).

### D3. Relleno de turnos acotado a los turnos nuevos de la pasada
- `Writer` registra, por sesión, la marca de tiempo mínima de los `Record::Turn` aplicados en el archivo/lote (`HashMap<session_id, min_ts>`, usando el `ts` efectivo tras `MIN(ts, excluded.ts)`). `backfill_turns` solo recorre esas sesiones: `UPDATE {tabla} SET turn_id = (…) WHERE session_id = ?1 AND turn_id IS NULL AND ts >= ?2`.
- **Equivalencia**: una fila con `turn_id NULL` lo tiene porque al insertarla no había ningún turno de su sesión con `ts <= fila.ts` (el `INSERT` ya lo busca) y ningún relleno posterior lo encontró. Solo puede ganar turno si llega un turno de su sesión con `ts <= fila.ts`, es decir, un `Record::Turn` de esta pasada con `ts_turno <= fila.ts`; esas filas cumplen `ts >= min_ts`. Las filas sin turno de sesiones sin turnos nuevos no pueden cambiar, así que saltarlas no altera el resultado. Lo verifica la huella (D1) y un test específico del caso subagente-antes-que-sesión.
- **Por qué**: con líneas sin prompt (refresco típico de una sesión larga y peor caso medido) no se lanza ningún `UPDATE`; el coste deja de crecer con la sesión.
- **Alternativa**: índice parcial `(session_id, ts) WHERE turn_id IS NULL` (lo propone la línea base para la Fase 2; acelera el recorrido pero no lo evita; queda fuera de alcance).

### D4. Pipeline de escaneo: planificar → leer/parsear en paralelo → escribir en orden
```
plan (hilo llamador)        workers (thread::scope, N)          escritor (hilo llamador)
register_agents, DELETE  →  i = next.fetch_add(1)            →  BTreeMap<i, ParsedFile> reordena
walk + filtro matches       espera si i >= escrito + VENTANA     aplica i = 0,1,2… en lotes:
carga file_state (1 SELECT) lee desde offset, parsea, envía      BEGIN; por archivo SAVEPOINT,
lista ordenada de trabajos  (i, Result<ParsedFile>) por          Writer + backfill + file_state;
(proveedor, raíz, ruta)     mpsc::sync_channel(VENTANA)          COMMIT cada ~5 000 registros
```
- **Orden determinista**: la lista de trabajos sigue exactamente el orden actual (proveedores en el orden recibido, raíces en orden, `walk` ordenado). El escritor aplica en ese índice; así la asignación de turnos (dependiente del orden, ver Context) es idéntica a la secuencial.
- **Paralelismo**: `N = min(available_parallelism, nº de archivos pendientes)`, mínimo 1; `std::thread::scope` (los workers toman `&[Box<dyn Provider>]` prestado sin `Arc`). Ventana `VENTANA = 4 × N`: un worker no reclama el índice `i` hasta que `i < escrito + VENTANA` (`Mutex<usize>` + `Condvar`); con ello la memoria en vuelo está acotada y el índice que espera el escritor siempre es reclamable (sin interbloqueo).
- **Estado por archivo**: cada archivo lo procesa un solo worker, de principio a fin, así que el estado por ruta de los proveedores (`reset`/`knows`/`parse_line`) se respeta; los `Mutex` de los proveedores solo protegen su mapa. OpenCode (`Source::Sqlite`) se procesa también como trabajo: su `read_db` se hace en el worker y el escritor aplica los registros igual que hoy.
- **Lote y bloqueo**: el escritor recibe un «sumidero» que abre la conexión para cada lote. Dos implementaciones: `&mut Connection` (MCP, `perf_scan`, tests: `scan_all` conserva su firma) y `&Mutex<Connection>` (hilo de ingesta: bloquea solo para escribir cada lote y para la carga inicial de `file_state`). Un lote son archivos completos hasta ~5 000 registros (constante, ajustable por medición) para que el `Mutex` se suelte cada pocos ms y los comandos intercalen.
- **Atomicidad por archivo**: `SAVEPOINT` por archivo dentro del lote; un error de escritura de un archivo deshace solo ese archivo, se cuenta como error con `tracing::warn!` y la ruta (como hoy), y el lote sigue.
- **Estadísticas y spans**: `ScanStats` y los spans `scan_all`, `walk`, `ingest_file`, `ingest.tx` de la Fase 0 se conservan (el span `ingest_file` pasa a cubrir lectura+parseo en el worker; `ingest.tx` cubre la escritura del archivo).
- **Alternativas**: `rayon` (`par_iter` + `collect` ordenado: espera a todo antes de escribir o necesita el mismo reordenado; dependencia nueva sin ventaja con `thread::scope`); escritura en paralelo (SQLite tiene un único escritor); trozos fijos parse-luego-escribe (no solapa parseo con escritura, que es el 83 %).

### D5. Lectura incremental robusta
- `file_state` se carga una vez por escaneo en `HashMap<String, (file_id, size, offset, mtime)>` (un `SELECT`), y se pasa a los workers por referencia (solo lectura). Las decisiones son las de hoy: misma identidad (`dev:ino` en Unix) y `size >= offset` → continuar desde el offset si el proveedor `knows(path)`; si no, `reset` y desde 0; `size == offset` con misma identidad → saltar **sin abrir** el archivo.
- Lectura con `BufReader::with_capacity(64 KiB)` + `read_until(b'\n')` desde el offset; una línea sin `\n` final no se consume y el offset avanza solo por líneas completas; los documentos completos (`whole_file`) se leen enteros como hoy. Cada línea se convierte con `String::from_utf8_lossy` (sin copia si es UTF-8 válido).
- Se guarda `(ruta, identidad, tamaño del stat, offset, mtime)` igual que hoy (mismo esquema).
- **Alternativa**: `mmap` (dependencia y riesgos con archivos que crecen/truncan durante la lectura).

### D6. Hilo de ingesta dedicado
- Módulo nuevo `src-tauri/src/ingest_thread.rs` (infraestructura): `IngestThread::spawn(db: Arc<Mutex<Connection>>, providers: Vec<Box<dyn Provider>>, on_done: impl Fn(Origen, &ScanStats) + Send) -> IngestHandle`; `IngestHandle::request(Origen)` envía `Job::Scan(Origen)` por `std::sync::mpsc::Sender`. `Origen = EscaneoInicial | Vigilante`.
- El hilo posee **una sola** colección de proveedores para toda la vida de la app (escaneo inicial y vigilante comparten estado: los proveedores con estado ya no releen desde 0 en el primer refresco). Si hay varios `Job::Scan` en cola se drenan y se hace un único escaneo (el escaneo es completo e idempotente); el origen inicial tiene prioridad para el aviso.
- `on_done` reproduce los avisos actuales: escaneo inicial → `ingest://done` con `ScanStats` y refresco de bandeja/avisos; vigilante → `ingest://done` sin datos y refresco solo si `records > 0`. Un error de escaneo se registra con `tracing::error!`/`warn!` como hoy y el hilo sigue vivo.
- `watcher::spawn(roots: Providers, ingest: IngestHandle)`: conserva la detección de carpetas, el debounce y el `RECHECK`; en vez de llamar a `scan_all` llama a `ingest.request(Origen::Vigilante)`. La fábrica de proveedores solo se usa para descubrir raíces.
- `lib.rs` crea el hilo de ingesta, encola `EscaneoInicial` y pasa el handle al vigilante.
- **Alternativas**: mantener el escaneo dentro del vigilante (dos hilos escribiendo, estado de proveedores duplicado); un runtime async para la ingesta (I/O de archivos y SQLite son bloqueantes: no aporta).

### D7. Comandos Tauri `async`
- Todos los `#[tauri::command]` de `commands.rs` pasan a `async fn` con `tauri::State<'_, AppState>`; `with_db` pasa a `async fn with_db(db: Arc<Mutex<Connection>>, f) -> CmdResult<T>` que ejecuta `f` en `tauri::async_runtime::spawn_blocking` (bloqueo del `Mutex` y consulta fuera del hilo principal y del ejecutor async). `get_settings`, `list_prices` y `set_settings` hacen `settings::load/save` también dentro de `spawn_blocking`.
- Mismos nombres, argumentos, tipos de respuesta y mensajes de error (`CmdResult<T> = Result<T, String>`); spans `cmd.<nombre>` conservados (`#[tracing::instrument]` admite `async fn`). `tray::show_main` no se toca (no hace I/O de disco).
- Test: la ayuda `run_blocking` (núcleo de `with_db` sin `tauri::State`) ejecuta el cierre en un hilo distinto del llamador y propaga errores con el mismo formato `{e:#}`.

### D8. Parseo mínimo con sonda de tipo
- Por proveedor JSONL, una sonda `#[derive(Deserialize)] struct Probe<'a>` con solo los campos de envoltura (`type`/`subtype`, sesión, fecha y, en Claude Code, `cwd`/`gitBranch`), con `#[serde(borrow)] Cow<'a, str>` (los `&str` prestados fallan con secuencias de escape). Sin `deny_unknown_fields`: los campos no declarados se saltan sin construir `Value`.
- Regla de descarte: una línea se descarta tras la sonda **solo si** su parseo completo actual no produce registros **ni** cambia el estado del proveedor (p. ej. en Claude Code una línea sin `sessionId` o sin `timestamp` válido; en los proveedores con estado, los tipos que su `parse_line` ignora sin tocar estado). La lista exacta por proveedor se deriva de su `parse_line` actual en la tarea correspondiente y la guarda la huella (D1). En Claude Code toda línea con sesión y fecha produce `Record::Session` (afecta a `started_at`/`ended_at`), así que esas líneas no se descartan: se evita deserializar `message` salvo en `assistant`/`user`.
- Las líneas útiles se deserializan a structs con solo los campos que usa el proveedor; el texto de los mensajes se lee prestado (`Cow`) solo para `prompt_intent`/clasificación y nunca se guarda ni se copia. Se evita `#[serde(untagged)]` (bufferiza en `Content` y copia): para campos que pueden ser cadena o lista se usa un `Visitor` propio o `serde::de::IgnoredAny` cuando no se necesita el valor.
- Orden: Claude Code (60 % de los datos), Codex, Copilot, Cursor, Gemini; un paso por proveedor, cada uno con su bench `parse_line/<agente>` antes/después y la regla de D9. OpenCode (SQLite) no cambia.
- **Alternativas**: `memchr` sobre `"type":"…"` (frágil ante espacios/orden de claves y ya cubierto por la sonda serde; sería dependencia directa nueva); `simd-json` (dependencia pesada; el parseo es el 8,7 % del frío).

### D9. Medición por paso y regla de reversión
- Cada paso de optimización se mide con `perf_scan` en release sobre `200m` (frío; vivo con `--repeat 200` cuando el paso afecta al refresco) y, si aplica, con el bench criterion del componente. Tres ejecuciones; se compara la mediana con la del paso anterior y con `BASELINE.md`.
- Se conserva un paso si mejora su métrica objetivo ≥ 5 % (por encima del ruido observado de la máquina) sin empeorar otra > 5 %, o si es un requisito estructural (hilo de ingesta, comandos `async`, sin pánicos) que no empeora > 5 %. Si no, se revierte y se anota el motivo en «Desviaciones durante la implementación» de este documento y en `docs/perf/BASELINE.md`.
- `perf_scan` añade: `--fingerprint` (D1) y, en la fase `live`, la mediana de los primeros 20 y de los últimos 20 refrescos (criterio de «no crece» del spec).

### D10. Sin pánicos en hilos de fondo
- `alerts::today_start`: sin `expect`; si no se puede construir la medianoche, devuelve 0 como ya hace con `earliest()` fallido.
- `tz::day_start` / `tz::hour_start`: `timestamp_millis_opt(ts).earliest()`; si no hay representación local o no se puede construir la hora, devuelven `ts` sin truncar (mismo resultado para todas las entradas válidas; antes, pánico).
- Lints `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]` en `ingest.rs`, `ingest_thread.rs`, `watcher.rs`, `tz.rs`, `alerts.rs` y `providers/*` para que no vuelvan a entrar (los `unwrap_or*` siguen permitidos). Errores con `anyhow` + `tracing`; no hace falta `thiserror` (ningún llamador distingue variantes de error).

## Risks / Trade-offs

- [El paralelismo altera el orden de aplicación y con él los turnos asignados] → Escritura secuencial en el orden original (D4) + huella con turnos (D1) + test de ejecuciones repetidas.
- [Lotes de varios archivos en una transacción: un error deshace más de un archivo] → `SAVEPOINT` por archivo; test con un archivo que falla en escritura entre archivos válidos.
- [Interbloqueo en el reordenado si un archivo grande retrasa la ventana] → Reclamación de índices acotada por `escrito + VENTANA`; test con un archivo grande y muchos pequeños y `N = 2`.
- [Comandos que intercalan con la ingesta ven datos a medio escanear] → Ya ocurría entre archivos con el vigilante; cada lote se confirma entero y el spec de almacenamiento exige ver «los datos confirmados hasta ese momento». El aviso `ingest://done` sigue llegando al final.
- [Compartir proveedores entre escaneo inicial y vigilante cambia cuándo se relee desde 0] → Los datos son idempotentes por clave (spec de vigilancia); la huella tras escaneo + refresco se compara en el test del hilo.
- [Sonda de tipo descarta una línea que sí aportaba algo] → Regla de descarte estricta (D8), tests de cada proveedor existentes + huella con fixtures versionados y sintéticos.
- [Ruido de la máquina de medición (load average 31–52 en la Fase 0)] → Tres ejecuciones y umbral del 5 % (D9); se documentan las condiciones.
- [`backfill` sigue recorriendo la sesión cuando llega un turno nuevo] → Coste acotado a refrescos con prompt nuevo; el índice parcial queda para la Fase 2 (anotado como seguimiento).
- [Las comprobaciones de clave foránea (~14 % del frío) siguen] → Fuera de alcance; si D2 no basta para < 10 s se documenta y se propone para la Fase 2.

## Migration Plan

- Sin migración de datos ni de esquema (base en memoria, se rellena al arrancar).
- Despliegue normal con la siguiente versión; los eventos y comandos que ve el frontend no cambian.
- Rollback: revertir los commits de la fase; cada paso es un commit independiente y revertible.

## Open Questions

- Ninguna que cambie specs, enfoque o tareas. Decisiones tomadas de forma autónoma: sin dependencias nuevas (`std::thread::scope` + `mpsc` en vez de `rayon`; sonda serde en vez de `memchr`; `anyhow` en vez de `thiserror`), orden de escritura idéntico al secuencial (D4), un único conjunto de proveedores en el hilo de ingesta (D6), lote de ~5 000 registros y ventana `4 × N` como valores iniciales ajustables por medición, umbral de mejora del 5 % (D9), fallback de `tz` a `ts` sin truncar (D10), y el «no crece» medido como mediana de los últimos 20 refrescos ≤ 1,5 × la de los primeros 20.

## Dependencias añadidas (justificación)

- Ninguna. `std::thread::scope` cubre el paralelismo acotado sin `rayon`; la sonda con `serde` (ya presente) cubre el descarte temprano sin `memchr`; `anyhow` + `tracing` (ya presentes) cubren los errores sin `thiserror`.

## Desviaciones durante la implementación

- Partida: pendiente (tarea 1.6: huella de `200m` y cifras de partida).
