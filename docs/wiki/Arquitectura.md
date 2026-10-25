# Arquitectura — cómo se montó

AgentBoard se construyó con **Tauri 2**: un backend en **Rust** que hace todo el trabajo de
lectura y agregación, y una interfaz en **React + TypeScript + Vite**. El resultado es un único
binario nativo para Linux, Windows y macOS.

## Backend (Rust, `src-tauri/`)

- **Almacenamiento (`db.rs`).** Los datos van a una **caché SQLite reconstruible** en la carpeta
  de datos del usuario (`cache.sqlite`, en WAL, carpeta `0700` y archivos `0600` en Unix) o, con
  «Solo memoria», a una base en memoria con caché compartida. Al abrir la caché se comprueban su
  versión de esquema (`PRAGMA user_version`) y su marca; si no es válida, no abre o está dañada,
  se borra y se reconstruye leyendo los logs (no se migra). Si no se puede crear en disco, la
  sesión sigue en memoria. Un error de corrupción en ejecución pide una reconstrucción.
- **Escritor único y lectores aparte.** El hilo de ingesta (`ingest_thread.rs`) es el único que
  escribe: escaneos, cambios de precios y reconstrucciones, en transacciones por lote (5 000
  filas o 200 ms). Los comandos, la bandeja y los avisos leen por un conjunto de 2 conexiones de
  solo lectura (`ReadPool`): en disco no esperan al escritor ni al revés; en memoria, como mucho
  un lote o una consulta. Cada `COMMIT` sube un contador de generación de datos.
- **Columnas precalculadas y agregados (`derived.rs`).** Cada llamada guarda su coste, ahorro por
  caché y día local; cada sesión, su primera llamada. Los agregados `daily_stats` (día, sesión,
  modelo) y `tool_stats` (día, sesión, herramienta) se rehacen al final de cada lote solo para las
  claves que tocó. El Resumen y los apartados leen de ellos cuando el periodo son días locales
  completos de la zona de la caché; si no, de las llamadas, con el mismo resultado.
- **Caché de resultados (`result_cache.rs`).** Las consultas del Resumen y los apartados se
  guardan en memoria por consulta y filtro mientras no cambie la generación de datos.
- **Caché fiel a los logs.** Cada archivo leído recuerda a qué sesiones aportó; si desaparece, se
  sustituye o encoge, sus datos se retiran y esas sesiones se rehacen con sus otros archivos.
- **Providers.** Cada agente es un módulo en `providers/` que implementa el trait `Provider`:
  detecta si el agente está instalado, dice dónde están sus logs y parsea sus registros a unos
  tipos comunes (sesiones, llamadas, turnos, usos de herramienta). Las fuentes pueden ser JSONL
  (Claude Code, Codex, Copilot, Cursor, Gemini) o SQLite (OpenCode).
- **Ingesta incremental.** `ingest.rs` recuerda por qué offset va cada archivo (también entre
  arranques, en la caché), así que releer solo procesa lo nuevo; cada llamada se identifica por su
  `message_id` para no duplicar.
- **Consultas.** `queries.rs` resuelve las cifras del panel con agregaciones SQL (por agente,
  modelo, proyecto, actividad, herramienta…), siempre respetando el filtro común.
- **Precios.** `pricing.rs` guarda el precio por modelo (USD por millón de tokens) y calcula
  coste y ahorro por caché. Los modelos sin precio conocido cuentan como 0.
- **Vigilante en vivo.** `watcher.rs` usa el crate `notify` para observar las carpetas de logs.
  Descarta los eventos de solo acceso y las rutas que ningún provider reconoce (los `-wal`/`-shm`
  de una base SQLite cuentan como la base), agrupa los eventos **por archivo** (150 ms de calma,
  1 s como máximo para un log que no para de crecer) y pide al hilo de ingesta la **relectura
  dirigida** de esos archivos (`ingest::scan_paths`: misma tubería que el escaneo completo, sin
  recorrer las raíces; la limpieza de archivos borrados, rotados o truncados se aplica a ellos y a
  sus archivos relacionados). No hay sondeo: sin eventos ningún hilo se despierta. Una carpeta de
  logs que aún no existe se espera vigilando su antepasado existente más profundo; al aparecer se
  vigila entera y se leen los archivos que ya tenga. Solo una raíz que existe pero no se puede
  vigilar (p. ej. `max_user_watches`) se relee cada 30 s. Si el SO pierde eventos, escaneo
  completo.
- **Eventos a la interfaz.** Tras cada transacción con cambios, el hilo de ingesta pasa la
  generación y lo cambiado (agentes y días) al notificador (`notifier.rs`), que emite
  `data:changed { generation, agents, days }` como mucho una vez por segundo: el primero tras un
  segundo de calma sale al momento y los siguientes se agrupan. No lleva filas: la interfaz pide
  los datos que muestra. Listas vacías significan «puede haber cambiado todo» (precios, limpieza,
  reconstrucción). Con cada aviso se refrescan también la bandeja y los avisos de presupuesto.
  `ingest://done` (con sus estadísticas) queda solo para el fin del escaneo inicial y del de una
  reconstrucción, que es lo que espera «Reconstruir caché».
- **Trazas.** `telemetry.rs` configura `tracing`. Por defecto el nivel es `warn`: solo salen
  avisos y errores por stderr. Con la variable `AGENTBOARD_LOG` (sintaxis de `EnvFilter`, p. ej.
  `AGENTBOARD_LOG=debug` o `AGENTBOARD_LOG=agentboard_lib::ingest=debug`) se activan los spans
  del escaneo, la ingesta por archivo, la transacción de inserción, los comandos, las consultas
  y la emisión de `ingest://done` y `data:changed`, cada uno con su duración (`time.busy`). Un valor no válido
  se ignora (queda `warn`). Las trazas van siempre a stderr, nunca a stdout (el servidor MCP
  usa stdout para JSON-RPC).

## Frontend (React + TypeScript, `src/`)

- **Vistas y componentes.** `views/` arma el resumen y las vistas ampliadas; `components/`
  contiene el panel lateral, las tablas y los gráficos, dibujados a medida.
- **Internacionalización.** `lib/i18n.ts` usa el **texto en español como clave** y diccionarios
  para inglés, portugués y francés.
- **Temas.** El tema (claro, oscuro o del sistema) se resuelve por un atributo en el documento;
  el modo oscuro usa un acento naranja y el claro uno celeste.
- **Caché de datos (`lib/query.ts`).** Cada consulta se guarda por comando y filtro
  (`filterKey`) con la generación de datos en la que se pidió. Los avisos `data:changed` e
  `ingest://done` y guardar los ajustes cambian la generación: solo se vuelven a pedir las
  consultas de las vistas montadas; las demás se piden al volver a mostrarse, enseñando antes el
  dato anterior. Las peticiones iguales simultáneas comparten una sola llamada, no hay recarga al
  recuperar el foco y la memoria se acota con un LRU de 64 entradas.
- **Filtro global (`lib/filterStore.ts`).** Periodo, agentes y proyectos ocultos y comparación
  viven fuera de React; la barra superior, el panel lateral y el contenido se suscriben solo a lo
  que usan (`useSyncExternalStore`). El contenido usa el filtro diferido (`useDeferredValue`): la
  barra responde al instante y el Resumen del filtro nuevo se pide ya en el render urgente.
- **Carga por apartado y tablas largas.** Los apartados distintos del Resumen se cargan con
  `React.lazy`; las tablas con más de 200 filas solo pintan las visibles más un margen
  (`lib/virtual.ts`).

## Especificación primero

El proyecto se desarrolla con **OpenSpec**: cada cambio se describe primero como propuesta y
especificación en `openspec/` antes de implementarse, y se archiva al completarse.

## Integración continua y distribución

- Cada Pull Request compila y prueba la app y el servidor MCP en **Linux, Windows y macOS**
  (matriz en GitHub Actions), además de comprobar formato, clippy, tipos y tests.
- Un tag `v*` dispara el workflow de release, que empaqueta los instaladores de cada sistema
  (`.AppImage`/`.deb`, `.msi`/`.exe`, `.dmg` universal) con `tauri-action`.
