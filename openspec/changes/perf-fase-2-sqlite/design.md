# Design

## Context

- Motivación: proposal.md (Why). Requisitos: `specs/almacenamiento-local`, `specs/ajustes`, `specs/rendimiento`. Cifras de partida: `docs/perf/BASELINE.md` («Después de la Fase 1», cuellos #2 y #4 y «Seguimientos para la Fase 2»).
- Estado actual observado:
  - `db.rs`: `open_in_memory` (WAL sin efecto en memoria, `synchronous=NORMAL`, `foreign_keys=ON`, caché de 64 sentencias); migraciones 0001–0004 por `PRAGMA user_version`; `remove_legacy_db` borra `agentboard.db`/`agentburn.db` y luego `remove_dir` de `~/.local/share/agentboard` (solo si queda vacía).
  - Esquema: `calls.ts` ya es epoch ms entero; vista `call_costs` = `SELECT c.*` + precio por subconsulta correlacionada `MAX(valid_from)` en cada fila leída. `calls` se escribe con `ON CONFLICT(message_id) DO UPDATE` (tokens, `ts = MIN(...)`, modelo): una llamada puede cambiar tras insertarse, así que un agregado por suma de deltas no es seguro.
  - Consultas (`queries.rs`, `insights.rs`, `sessions.rs`, `pricing::list_prices`): casi todas son `GROUP BY` sobre `call_costs JOIN sessions`; `breakdown` calcula `ROW_NUMBER() OVER (PARTITION BY session_id ORDER BY ts)` sobre todas las llamadas para el «overhead» de la primera llamada; `timeseries`/`timeseries_by` agrupan por día u hora local en Rust (`tz::day_start` con la zona IANA que manda la interfaz). Los filtros son `from`/`to` en ms (la interfaz manda medianoches locales) más agentes y proyectos, que viven en `sessions`; las sesiones se cuentan con `COUNT(DISTINCT session_id)` sin subagentes.
  - El Resumen (`useDashboardData`) hace 19–23 llamadas: `summary`, `timeseries(day|hour)`, `timeseries_by(agent|model|project|branch|tool)`, el mes (`timeseries` desde el inicio de mes), `breakdown(agent|project|branch|model|tool|command|skill|mcp|agent_type)`, `activity`, `activity_daily` y, con comparación, tres más del periodo anterior.
  - `pricing::apply_overrides` borra **todos** los precios y los vuelve a insertar; `lib.rs` lo llama en cada arranque y `set_settings` en cada guardado. Los tests de `pricing`, `queries::testdata::seed` y otros insertan llamadas con SQL directo y leen el coste de `call_costs`.
  - `commands.rs`: `AppState { db: Arc<Mutex<Connection>> }` compartido con `IngestThread` (escribe por lotes con `scan_shared`), `alerts::refresh` y `tray::refresh`; `set_settings` aplica precios con ese mismo `Mutex`.
  - Claves foráneas: el test `un_fallo_al_escribir_deshace_solo_ese_archivo` depende de que una llamada de una sesión inexistente falle al escribir.
  - Servidor MCP (`mcp/src/main.rs`): `db::open_in_memory` + `apply_overrides` + `ingest::scan_all` una vez al arrancar; su propio cambio (sin archivar) exige «no escribir ningún dato en disco».
  - Herramientas de la Fase 1 reutilizadas: `perf_scan` (`--phase cold|live|queries`, `--fingerprint`), `benches/summary_queries.rs`, `benches/common/mod.rs::fingerprint` (columnas explícitas), golden de ingesta en `tests/golden/`, fixtures `2m`/`50m`/`200m`/`1g`.

## Goals / Non-Goals

**Goals:**
- Cuellos #2 (consultas que recorren todo el historial) y #4 (contención del `Mutex` único) de la línea base, y arranque incremental con la caché en disco.
- Resultados de consultas idénticos, verificados con un golden grabado antes de tocar nada.
- Cero dependencias nuevas.

**Non-Goals:**
- Relecturas dirigidas por ruta en el vigilante, debounce y eventos `data:changed` (Fase 3); comando único `get_dashboard_snapshot` (Fase 3).
- Rendimiento del frontend (Fase 4); solo se añade la interfaz mínima de Ajustes.
- Leer la caché de la app desde el servidor MCP (D11).
- Migrar cachés de versiones de esquema anteriores (se reconstruyen, D2).

## Decisions

### D1. Golden de consultas y huella de consultas antes de optimizar
- Test `src-tauri/tests/equivalencia_consultas.rs`: escanea el fixture sintético `2m` (semilla 11, como `equivalencia_ingesta.rs`) y `tests/fixtures`; ejecuta con zona `Europe/Madrid` y `now` fijo todas las consultas que lanza el Resumen y los apartados (`summary`, `timeseries(day|hour)`, `timeseries_by` ×5, `breakdown` ×10 incluida `activity`, `activity`, `activity_daily`, `list_agents`, `list_projects`, `pricing::list_prices`, `sessions::list_sessions`, `sessions::list_projects`) para un conjunto fijo de filtros: todo; 30 días alineados a medianoche local; inicio de mes; 7 días con un agente; un proyecto; `no_project`; periodo anterior con `to`; y un `from` a mitad de día (ruta de detalle). Salida JSON canónica, una línea por (consulta, filtro), en `tests/golden/consultas_2m.txt` y `consultas_fixtures.txt`; `UPDATE_GOLDEN=1` los graba.
- Comparación: estructural, enteros y cadenas exactos, números reales con `|a − b| ≤ 1e-6` (sumar por agregados cambia el orden de las sumas en coma flotante). Se graban **en el commit de partida** y no se regraban en la fase. Los golden de ingesta existentes no se tocan.
- El test se ejecuta en modo memoria y en modo disco (D2) desde el paso en que exista el disco.
- `perf_scan --query-fingerprint`: mismo volcado sobre `200m`, resumido a un hash FNV-1a por consulta tras redondear reales a 1e-6; se guarda en la partida y se compara al final.
- **Alternativas**: comparar solo `summary` (no cubre desgloses ni series); volcar sin tolerancia (falsos fallos por orden de suma).

### D2. Almacenamiento: caché en disco o en memoria, validación y reconstrucción
- `db.rs` expone `Storage::open(mode) -> Storage { writer: Connection, readers: ReadPool, mode }` con `CacheMode::Disco(PathBuf) | CacheMode::Memoria`. Ruta por defecto `dirs::data_dir()/agentboard/cache.sqlite`; en Unix carpeta `0700` y archivos `0600` (el `umask` se fija antes de crear la base para que `-wal`/`-shm` nazcan igual).
- Apertura en disco: abrir, comprobar `PRAGMA user_version` = última migración y la tabla `cache_meta` (`format`, `tz`). Cualquier error (no abre, `SQLITE_NOTADB`, `SQLITE_CORRUPT`, versión distinta) → cerrar, borrar `cache.sqlite`, `-wal` y `-shm`, crear de cero y aplicar migraciones; aviso `tracing::warn!`. Sin `PRAGMA integrity_check` al arrancar (recorre toda la base: incompatible con el objetivo de 1,5 s); la corrupción que aparezca después la detectan las sentencias (abajo).
- Si no se puede crear en disco (permisos, disco lleno) → `CacheMode::Memoria` para esa sesión con aviso.
- Corrupción en ejecución: un error `SQLITE_CORRUPT`/`SQLITE_NOTADB` en el escritor o en un lector marca un indicador atómico; el hilo de ingesta lo atiende con `Job::Reconstruir` (D12). Nunca `panic`.
- `remove_legacy_db`: solo borra `agentboard.db`/`agentburn.db` y sus `-wal`/`-shm`; deja de llamar a `remove_dir`.
- Versión de esquema distinta → reconstruir (no migrar): es una caché; reconstruir es el camino ya probado (escaneo frío) y evita migraciones que nunca se ejercitan con datos reales. Las migraciones siguen aplicándose en orden sobre una base vacía (memoria, MCP, caché nueva).
- **Alternativas**: migrar cachés antiguas (código y tests que solo sirven para no releer los logs una vez por versión); `integrity_check` al arrancar (segundos con 1 GB).

### D3. PRAGMAs y claves foráneas
- Escritor en disco: `journal_mode=WAL`, `synchronous=NORMAL` (en WAL no corrompe; un corte de luz pierde como mucho los últimos lotes, y `file_state` va en la misma transacción que sus filas, así que se releen), `temp_store=MEMORY`, `cache_size=-65536` (64 MiB), `foreign_keys=OFF`, `wal_autocheckpoint` por defecto.
- Escritor en memoria: `synchronous=OFF`, `temp_store=MEMORY`, `foreign_keys=OFF`.
- Lectores: `query_only=1`, `temp_store=MEMORY`, `cache_size=-16384`, `busy_timeout` 5 s, misma capacidad de caché de sentencias (64).
- **Claves foráneas desactivadas** (~14 % del frío en la Fase 0, `sqlite3FkCheck` + `fkScanChildren`): el escritor ya inserta la sesión antes que sus filas (D2b de la Fase 1). Para conservar el comportamiento ante datos rotos, el `Writer` comprueba que la sesión de una llamada, herramienta, turno o evento existe (conjunto en memoria de sesiones vistas en el archivo + una búsqueda por clave primaria con `prepare_cached`, memorizada) y, si no, devuelve error: el `SAVEPOINT` del archivo se deshace igual que hoy (el test `un_fallo_al_escribir_deshace_solo_ese_archivo` sigue en verde sin cambios). Las restricciones `REFERENCES` se quedan en el esquema y un test comprueba `PRAGMA foreign_key_check` vacío tras escanear los fixtures. Paso medido con la regla de D13.
- **Alternativa**: mantener `foreign_keys=ON` (si el paso no mejora ≥ 5 % se revierte).

### D4. Escritor único, lotes por filas o tiempo y generación de datos
- El hilo de ingesta (`ingest_thread.rs`) pasa a ser **dueño** de la conexión de escritura (sin `Arc<Mutex<Connection>>` compartido con comandos, bandeja ni avisos). Trabajos: `Job::Scan(Origen)` (como hoy), `Job::AplicarPrecios(Vec<PriceOverride>, respuesta)`, `Job::Reconstruir(CacheMode, respuesta)`. La respuesta es un `mpsc::Sender<Result<(), String>>` que el comando espera dentro de `spawn_blocking`. Los `Scan` encolados se siguen agrupando; los demás trabajos se atienden en orden entre escaneos.
- Lote: archivos completos hasta 5 000 filas **o** 200 ms desde el inicio del lote, lo que llegue antes (hoy solo cuenta registros). Un archivo de más de 5 000 filas sigue yendo entero en su `SAVEPOINT`.
- `DataGeneration(AtomicU64)` compartido: el escritor lo incrementa **después** de cada `COMMIT` (lote, precios, limpieza, reconstrucción). Lo usan la caché de resultados (D8) y nada más en esta fase.
- `scan_all(&mut Connection, …)` conserva su firma (MCP, `perf_scan`, tests); `scan_shared(&Mutex<Connection>)` desaparece en favor de que el hilo posea la conexión.
- **Alternativa**: seguir con el `Mutex` del escritor compartido y añadir solo lectores (los comandos de precios seguirían bloqueando lotes y no habría un único dueño de las escrituras).

### D5. Lectores separados
- `ReadPool`: 2 conexiones de solo lectura (`Vec<Mutex<Connection>>`), se toma la primera libre con `try_lock` y, si todas están ocupadas, se espera a la primera. Dos bastan: los comandos ya van a `spawn_blocking` y la mayoría responde en milisegundos con D6–D8; el número es una constante ajustable por medición.
- Disco: conexiones normales al mismo archivo en WAL: ven el último lote confirmado y no bloquean ni son bloqueadas por el escritor.
- Memoria: escritor y lectores abren `file:agentboard-<pid>?mode=memory&cache=shared` (URI). En caché compartida, SQLite bloquea por tabla; con la feature `unlock_notify` de `rusqlite` (dependencia ya presente) una sentencia bloqueada espera a que se libere el bloqueo en lugar de fallar con `SQLITE_LOCKED`. Así la espera queda acotada a un lote o a una consulta y los lectores nunca ven lotes a medio escribir. El escritor mantiene viva la base en memoria mientras exista su conexión.
- `AppState` guarda `RwLock<Arc<ReadPool>>`: una reconstrucción (D12) sustituye el pool; las consultas en curso terminan con el anterior.
- Comandos, `alerts::refresh` y `tray::refresh` leen del pool. `with_db` pasa a `with_reader`.
- **Alternativas**: `read_uncommitted=1` en memoria (los lectores no esperan pero pueden ver lotes a medias y cachearlos); base temporal en disco para el modo memoria (contradice «solo memoria»); pool con crate (`r2d2`): dependencia sin ventaja para 2 conexiones.

### D6. Columnas precalculadas
- Migración 0005: `calls` añade `cost_usd REAL`, `cache_savings_usd REAL`, `has_price INTEGER`, `day INTEGER` (`YYYYMMDD` local); `tool_calls` añade `day INTEGER`; `sessions` añade `first_call_id TEXT`; tabla `cache_meta(key PRIMARY KEY, value)`.
- **Coste por disparadores**: la fórmula vive en un único sitio SQL, igual a la de la vista 0004 (mismo orden de operaciones, mismo resultado IEEE): `AFTER INSERT ON calls` y `AFTER UPDATE OF ts, model, input_tokens, output_tokens, cache_read, cache_write, cache_write_1h, cost_reported ON calls` actualizan las tres columnas de esa fila buscando el precio vigente por la clave primaria `(model, valid_from)`; `AFTER INSERT/UPDATE/DELETE ON prices` recalculan las llamadas del modelo afectado (índice `(model, ts)`). Así todos los caminos de escritura (escritor, tests con SQL directo, MCP) quedan bien sin cambiar sus llamadas.
- `call_costs` pasa a ser una vista **con columnas explícitas** sobre `calls` (sin `SELECT *` ni subconsultas), con los mismos nombres: ninguna consulta existente cambia de texto por esto.
- **Precios por diferencias**: `apply_overrides` calcula el conjunto deseado (por defecto + usuario) y solo borra/inserta las filas de `prices` que cambian. Un arranque sin cambios no dispara ningún recálculo; una versión con un precio por defecto nuevo recalcula solo ese modelo. Los agregados de los modelos afectados se rehacen en la misma transacción (D7).
- **Día local**: el `Writer` calcula `day` con `tz::system()` (la «zona de la caché», guardada en `cache_meta.tz`) y lo envía en el `INSERT`; en `ON CONFLICT` se toma el día de la `ts` que gana (`MIN`). Las filas escritas por otros caminos sin `day` (tests) o con `ts` cambiada (un disparador pone `day = NULL`) se completan en `derived::refresh`. Si al abrir la caché la zona del sistema no coincide con `cache_meta.tz`, se ponen todos los `day` a `NULL` y se rehacen todos los agregados (raro; segundos con 1 GB).
- **Primera llamada de la sesión**: `derived::refresh` recalcula `sessions.first_call_id` de las sesiones tocadas (`ORDER BY ts, rowid LIMIT 1` sobre las llamadas principales, índice `(session_id, is_sidechain, ts)`). `breakdown` sustituye el CTE `firsts` con `ROW_NUMBER` por `LEFT JOIN … ON s.first_call_id = c.message_id`. El desempate por `rowid` en llamadas con la misma `ts` hace determinista lo que hoy decide el orden del ordenador de SQLite; el golden (D1) lo verifica.
- **Alternativas**: coste calculado en Rust por el escritor (los tests y caminos con SQL directo quedarían sin coste); columnas generadas (SQLite no admite subconsultas en ellas); dejar la vista y cachear resultados (no arregla el frío).

### D7. Agregados diarios incrementales
- Tablas: `daily_stats(day, session_id, model, calls, cost_usd, cache_savings_usd, input_tokens, output_tokens, cache_read, cache_write, unpriced_calls, first_calls, first_ctx_tokens, min_ts, max_ts)` con PK `(day, session_id, model)`, y `tool_stats(day, session_id, tool, uses, errors, min_ts)` con PK `(day, session_id, tool)`. Agente, proyecto, rama y subagente se obtienen por `JOIN sessions` (pocas filas).
- **Por qué un grano por sesión y no `daily_agent_stats`/`daily_project_stats`/`daily_model_stats` por separado**: el filtro combina periodo, agentes **y** proyectos a la vez (una tabla por dimensión no puede filtrarse por la otra), y las sesiones se cuentan con `COUNT(DISTINCT)` (no se pueden sumar entre días ni entre dimensiones). Con el grano `(día, sesión, modelo)` todas las vistas de agente, proyecto, rama y modelo salen de una sola tabla, exactas, con `GROUP BY` sobre unas miles de filas en vez de cientos de miles. Un desglose por actividad necesitaría otra dimensión; se queda en la ruta de detalle (no es del Resumen salvo `activity`, que usa turnos).
- **Mantenimiento**: disparadores `AFTER INSERT/UPDATE/DELETE` en `calls` y `tool_calls` anotan `(session_id, day)` (de `NEW` y `OLD`) en `dirty_keys`; `derived::refresh(conn)` (al final de cada lote, en la misma transacción, antes del `COMMIT`): completa `day` nulos, recalcula `first_call_id` de las sesiones sucias (si cambia, ensucia también el día de la primera anterior), y para cada clave sucia hace `DELETE` + `INSERT … SELECT … GROUP BY` desde `calls`/`tool_calls` con el índice `(session_id, day)` (un UPSERT completo de la clave, no una suma de deltas: las llamadas pueden cambiar tras insertarse). Vacía `dirty_keys`. `apply_overrides` y la limpieza de archivos (D10) llaman a `refresh` igual. Los helpers de test que siembran con SQL directo (`queries::testdata::seed`, `pricing` tests) llaman a `refresh` al final; sus aserciones no cambian.
- **Ruta de consulta**: `queries` decide por consulta: si la zona pedida coincide con la de la caché y `from`/`to` son medianoches locales de esa zona (o no existen), lee de `daily_stats`/`tool_stats` con `day BETWEEN`; si no, ruta de detalle sobre `calls` (ya sin subconsulta de precio). Pasan a agregados: `summary` (salvo el `burn rate` de la última hora, que es detalle por índice de `ts`), `timeseries(day)` (el inicio del cubo es `tz::day_start(MIN(min_ts))` del día: el mismo valor que hoy), `timeseries_by(agent|model|project|branch|tool)`, `breakdown(agent|project|branch|model|tool|mcp)`, `list_agents`, `list_projects`. Siguen en detalle: `timeseries(hour)`, `activity`, `activity_daily`, `breakdown(activity|command|skill|agent_type)`, sesiones, proyectos y precios; todas se benefician de D6 y D9.
- **Alternativas**: deltas por disparador (no exactos con `UPSERT` que reemplaza tokens, ni con `MIN`/`MAX`/primeras llamadas, y la coma flotante deriva); vistas materializadas por dimensión (ver arriba); claves sucias en Rust dentro del `Writer` (más rápido si los disparadores pesan, pero deja sin cubrir precios, limpieza y tests; queda como alternativa medida si el paso empeora el frío > 5 %).

### D8. Caché de resultados
- Módulo `result_cache.rs`: `ResultCache { generation: u64, entries: HashMap<(QueryKind, String), Arc<dyn Any + Send + Sync>> }` tras un `Mutex`. Clave = tipo de consulta + JSON canónico de sus argumentos (filtro, `by`, `bucket`, `tz`) y, en las que dependen de la hora (`summary` por el `burn rate`, el mes), el minuto actual.
- Lectura: si la generación actual (D4) es distinta de la guardada, se vacía todo (solo se guardan resultados de la generación vigente). Escritura: se anota la generación antes de consultar y se guarda solo si sigue igual al terminar (un lote confirmado durante la consulta impide cachear un resultado que podría no incluirlo). Tope de 256 entradas: al llenarse se vacía.
- La usan los comandos del Resumen y de los apartados (`get_summary`, `get_timeseries`, `get_timeseries_by`, `get_breakdown`, `get_activity`, `get_activity_daily`, `list_agents`, `list_projects`); el resto no. El MCP no la usa (proceso de vida corta).
- **Alternativas**: LRU con dependencia (`lru`); cachear JSON serializado (cambia los tipos de retorno de los comandos).

### D9. Índices y `EXPLAIN QUERY PLAN`
- Índices iniciales (migración 0005, se ajustan con el plan y la medición): `calls(session_id, day)` y `tool_calls(session_id, day)` (agregados por clave sucia); `calls(model, ts)` (recálculo de precios, desglose por modelo en detalle); `calls(session_id, is_sidechain, ts)` (primera llamada); índices parciales `calls(session_id, ts) WHERE turn_id IS NULL` y el mismo en `tool_calls` (relleno de turnos, seguimiento de la Fase 1); `daily_stats(session_id)` y `tool_stats(session_id)` (limpieza); `dirty_keys` con PK. Un índice existente que ningún plan use tras el cambio (p. ej. `calls_ts`, que sigue haciendo falta para el `burn rate` y las rutas de detalle) solo se elimina si el test de planes lo confirma.
- Los filtros de agente y proyecto están en `sessions`, no en `calls`: no se añaden `(agent, ts)` ni `(project, ts)` sobre `calls` (sería desnormalizar); el plan de las rutas de detalle es «buscar `calls` por `ts` y unir `sessions` por clave primaria», que el test comprueba.
- Test `consultas_usan_indices` (en `queries.rs`): para cada consulta del Resumen con el filtro de 30 días y con agente/proyecto, `EXPLAIN QUERY PLAN` no contiene `SCAN calls` ni `SCAN tool_calls` sin índice; el plan de cada consulta se anota en «Desviaciones».
- Sin conversión de fechas a texto en SQL (no la hay hoy) ni `SELECT *` en vistas (D6).

### D10. Caché fiel a los logs
- Tabla `session_files(session_id, file_key)` con `file_key` = id estable del archivo: la migración 0005 recrea `file_state` con `id INTEGER PRIMARY KEY` y `path TEXT UNIQUE` copiando sus filas (el `rowid` implícito de hoy no es estable ante `VACUUM`); la migración se aplica en orden también sobre bases con datos, como prueba `base_de_fase_0_migra_sin_perder_datos`, y rellena `day`, coste y agregados de las filas existentes. El `Writer` inserta `(sesión, archivo)` una vez por par y archivo (`INSERT OR IGNORE`, memorizado) para **todo** registro que lleve sesión.
- Al planificar un escaneo, un archivo de `file_state` que ya no existe o que no aparece en el recorrido de su raíz, o que cambió de identidad o encogió (los casos que ya hoy releen desde 0), dispara la **limpieza**: sesiones afectadas = las de `session_files` de ese archivo; archivos relacionados = los demás archivos de esas sesiones. En una transacción del escritor: borrar `calls`, `tool_calls`, `turns`, `events`, `daily_stats`, `tool_stats`, `session_files` y `sessions` de las sesiones afectadas; borrar el `file_state` del archivo desaparecido y **reiniciar a offset 0** el de los relacionados (y `reset` de su estado en el proveedor); proyectos huérfanos como hoy. El escaneo sigue y relee los archivos relacionados enteros, rehaciendo las sesiones con exactamente sus datos (las fechas `MIN`/`MAX` no pueden encoger sin borrar la sesión: por eso se borra entera).
- Se aplica en todos los escaneos y modos (arranque y vigilante), para que «leer desde cero» y «escaneo incremental» den siempre lo mismo. Test: escanear A∪B, borrar B (y un caso con una sesión repartida entre A y B), reescanear → huella igual a escanear solo A en una base nueva.
- **Alternativas**: reconstrucción completa cuando falta algún archivo (Claude Code borra logs antiguos a menudo: anularía el arranque incremental); no limpiar (la caché mostraría historial que una reconstrucción perdería: los números dependerían de cuándo se reconstruyó).

### D11. Servidor MCP sin cambios
- Sigue con `db::open_in_memory` (escritor en memoria sin caché compartida, que no necesita lectores) + `apply_overrides` + `scan_all`. Como los agregados, el coste precalculado y la limpieza viven en la ingesta y el esquema, el MCP los aprovecha sin cambiar su código; su requisito de no escribir en disco se mantiene.
- **Por qué no abrir la caché de la app en solo lectura**: acoplaría dos binarios a la misma versión de esquema (un MCP más antiguo que la app vería una caché «de otra versión» y no podría reconstruirla sin escribir), WAL exige escribir `-shm` incluso para leer, y con «Solo memoria» no habría caché. Una conexión MCP es corta; el escaneo en memoria ya es de 2,7 s para 200 MB.

### D12. Ajustes «Solo memoria» y «Reconstruir caché»
- `Settings` añade `memory_only: bool` (`#[serde(default)]`, por defecto `false`; `memoryOnly` en la interfaz).
- Comando nuevo `rebuild_cache` → `Job::Reconstruir(modo actual)`. `set_settings` con `memory_only` cambiado → `Job::Reconstruir(nuevo modo)`. Reconstruir: crear `Storage` nuevo (disco vacío tras borrar los archivos, o memoria), aplicar precios, sustituir el pool de lectores, vaciar la caché de resultados (generación +1), soltar el almacenamiento anterior y borrar sus archivos si pasa a memoria, y encolar un escaneo completo (`Origen::EscaneoInicial`, que emite `ingest://done` con sus `ScanStats` al terminar). En Windows, si el borrado falla porque un lector aún tiene el archivo abierto, se reintenta en el siguiente arranque: con «Solo memoria» activa, el arranque borra cualquier `cache.sqlite` que encuentre.
- Interfaz: pestaña «Datos» en `SettingsDialog` con el interruptor «Solo memoria» (explicación: «No guarda la caché en disco; al abrir la app se vuelven a leer todos los logs») y el botón «Reconstruir caché» (deshabilitado desde que se pulsa hasta recibir `ingest://done`). `api.ts` añade `rebuildCache()`; `mock.ts` y el estado inicial de `App.tsx` añaden `memoryOnly: false`; textos en `i18n.ts` para `en`, `pt` y `fr` (el español es la clave).

### D13. Medición por paso y regla de reversión
- `perf_scan` añade: `--cache <ruta>` (escribe en disco en lugar de memoria); `--phase warm` (abre una caché ya llena, aplica precios, ejecuta el Resumen completo y después el escaneo incremental; imprime «arranque hasta datos del Resumen»); en `--phase queries`, el Resumen completo en frío y repetido a través de la caché de resultados, y cada consulta por separado; `--phase contention` (un hilo repite el Resumen por el pool de lectores mientras el escritor hace 200 refrescos de 1000 líneas; imprime la mediana del refresco con y sin lecturas); `--query-fingerprint` (D1).
- Cada paso: `perf_scan` en release, tres ejecuciones (o A/B alternando binarios si la carga varía, como en la Fase 1), sobre `200m` y, en los pasos de consultas, `1g`. Se conserva si mejora su métrica ≥ 5 % sin empeorar otra > 5 %, o si es estructural (escritor dueño, lectores, persistencia, limpieza, ajustes) y no empeora > 5 %; si no, se revierte y se anota en «Desviaciones durante la implementación» y en `docs/perf/BASELINE.md`.

### D14. Documentación
- `README.md` (insignia, «Sin persistencia» → «Caché local reconstruible», tabla de tecnología, diagrama), `CONTRIBUTING.md` (filosofía: «solo lectura de los logs; caché local reconstruible», dónde está la caché y cómo borrarla), `docs/wiki/Home.md`, `Arquitectura.md` (almacenamiento, escritor/lectores, agregados, caché de resultados), `Funcionalidad.md` (Ajustes → Datos) y `Servidor-MCP.md` (sigue en memoria, sin escribir en disco).

## Risks / Trade-offs

- [Los disparadores de coste y claves sucias encarecen cada inserción y empeoran el frío] → Paso medido (D13); alternativa preparada: claves sucias en Rust en el `Writer` y coste en el propio `UPSERT`, con `refresh_all` para tests. El presupuesto de frío (< 10 s, hoy 2,7 s) deja margen.
- [El orden de suma en coma flotante cambia los importes en el último bit] → Golden con tolerancia 1e-6 USD (D1); la huella de ingesta sigue usando la vista sobre llamadas (misma suma que hoy).
- [Empates de `ts` en la primera llamada de una sesión: hoy los decide el orden interno de SQLite] → Desempate por `rowid` y golden; si el golden difiere en `overhead_tokens`, se documenta el caso y se elige el desempate que reproduce el anterior.
- [Agregados desincronizados por un camino de escritura olvidado] → Disparadores en las tablas (no en el `Writer`) + test de propiedad: tras secuencias aleatorias de inserciones, actualizaciones, cambios de precio y limpiezas, `refresh` deja los agregados iguales a reconstruirlos desde cero.
- [La caché en disco expone comandos de shell y rutas de proyectos a otros usuarios] → Permisos `0600`/`0700` en Unix; solo datos que ya hay en los logs del propio usuario; «Solo memoria» para quien no quiera nada en disco.
- [Corrupción o versión incompatible] → Borrar y reconstruir (D2); nunca `panic` (lints de la Fase 1 en los módulos nuevos).
- [Caché compartida en memoria: esperas entre lectores y escritor] → Acotadas por lotes de ≤ 200 ms y consultas rápidas tras D6–D8; es el modo opcional.
- [Limpieza por archivo desaparecido relee sesiones enteras de otros archivos] → Solo las sesiones afectadas; el caso común (un archivo por sesión) no relee nada más.
- [Cambio de zona horaria recalcula todo] → Raro; se hace en el escritor con la interfaz respondiendo por la ruta de detalle mientras tanto (la zona de la caché no coincide con la pedida).
- [Escaneo frío en disco más lento que en memoria] → Medido; `synchronous=NORMAL` en WAL y lotes grandes; presupuesto de 10 s.
- [Ruido de la máquina de medición] → Tres ejecuciones o A/B, umbral del 5 % (D13).

## Migration Plan

- Primer arranque de la versión nueva: no hay caché → se crea y se hace un escaneo frío (igual que hoy). Las bases heredadas `agentboard.db`/`agentburn.db` se siguen borrando.
- Versiones posteriores con otro esquema: la caché se reconstruye sola (D2).
- Rollback: una versión anterior ignora `cache.sqlite` (usa memoria) pero su `remove_legacy_db` intentaría `remove_dir` de la carpeta, que falla al no estar vacía: el archivo queda en disco; se documenta en el README cómo borrarlo. Cada paso es un commit independiente y revertible.

## Open Questions

- Ninguna que cambie specs, enfoque o tareas. Decisiones tomadas de forma autónoma: grano `(día, sesión, modelo)` en lugar de tablas por dimensión (D7); coste y claves sucias por disparadores (D6, D7); reconstruir en vez de migrar cachés (D2); sin `integrity_check` al arrancar (D2); claves foráneas desactivadas con comprobación en el escritor (D3); 2 lectores (D5); `unlock_notify` en modo memoria (D5); limpieza por sesión ante archivos desaparecidos o rotados, en todos los escaneos (D10); MCP sin cambios (D11); reconstrucción inmediata al cambiar «Solo memoria» (D12); tope de 256 entradas en la caché de resultados (D8); tolerancia de 1e-6 USD en el golden (D1).

## Dependencias añadidas (justificación)

- Ninguna. Se activa la feature `unlock_notify` de `rusqlite` (ya dependencia) para que la caché compartida en memoria espere a los bloqueos en lugar de fallar; `dirs`, `serde_json`, `std::sync` y `tracing` ya están.

## Desviaciones durante la implementación

<!-- Se completa durante la implementación: cifras por paso, pasos revertidos y cambios respecto a D1–D14. -->
