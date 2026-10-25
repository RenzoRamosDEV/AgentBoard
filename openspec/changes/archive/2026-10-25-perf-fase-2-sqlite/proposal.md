# Proposal

## Why

Tras la Fase 1 la ingesta cumple sus presupuestos, pero las consultas no (`docs/perf/BASELINE.md`, «Después de la Fase 1»): el Resumen completo tarda 1,12–1,15 s con 200 MB y 6,2–7,8 s con 1 GB; `breakdown` (un `ROW_NUMBER` sobre todas las llamadas más la subconsulta de precio correlacionada de la vista `call_costs`) es el 54 % del tiempo de consultas; y el `Mutex` único hace que las tandas de consultas del Resumen retrasen decenas de segundos las relecturas del vigilante. Además, cada arranque relee todos los logs desde cero (2,7 s con 200 MB, 17,9 s con 1 GB) antes de que el Resumen tenga datos completos. El usuario ha aprobado guardar la base como caché local reconstruible para que el arranque sea incremental.

## What Changes

- **Caché local reconstruible en disco** (**BREAKING** respecto a «sin persistencia»): la base pasa a `cache.sqlite` en la carpeta de datos del usuario (`~/.local/share/agentboard/` en Linux), en modo WAL, con el estado de lectura de cada archivo de log, de modo que el arranque solo lee lo nuevo. Solo guarda lo que hoy está en memoria (nunca el texto de prompts ni respuestas), con permisos solo para el usuario. Si está dañada, no se puede abrir o tiene otra versión de esquema, se borra y se reconstruye sin cerrar la app. La limpieza de bases antiguas deja de borrar la carpeta de datos (solo borra `agentboard.db`/`agentburn.db`).
- **Caché fiel a los logs**: cuando un log desaparece, se sustituye o encoge, sus datos se retiran de la caché (y las sesiones afectadas se releen de sus otros archivos), de modo que el resultado es el mismo que leer los logs desde cero.
- **Ajuste «Solo memoria»** (por defecto desactivado) y botón **«Reconstruir caché»** en Ajustes, con textos en los cuatro idiomas de la interfaz.
- **Escritor único y lectores separados**: todas las escrituras (ingesta, precios, limpieza, reconstrucción) pasan por el hilo de ingesta en transacciones por lote (cada 5 000 filas o 200 ms, lo que llegue antes), con PRAGMAs de rendimiento; la interfaz, la bandeja y los avisos leen por conexiones de solo lectura propias, así que ni las consultas esperan al escritor ni el escritor a las consultas.
- **Columnas precalculadas**: coste, ahorro por caché y «tiene precio» por llamada, día local entero (`YYYYMMDD`) y primera llamada de cada sesión se guardan al escribir; desaparecen el `ROW_NUMBER` sobre todas las llamadas y la subconsulta de precio correlacionada. Un cambio de precios (del usuario o por defecto en una versión nueva) recalcula solo los modelos afectados, sin releer logs.
- **Agregados diarios incrementales** actualizados en la misma transacción de cada lote; Resumen, Daily Activity, By Agent, By Project y By Model leen de ellos cuando el periodo son días locales completos, con resultados idénticos a los de las llamadas.
- **Índices compuestos revisados con `EXPLAIN QUERY PLAN`** según los filtros reales; sin `SELECT *` en las vistas.
- **Caché de resultados en memoria** por consulta y filtro, invalidada por un contador de generación de datos que avanza con cada lote confirmado; memoria acotada.
- **Equivalencia y medición**: golden de las salidas de las consultas del Resumen grabado antes de optimizar; `perf_scan` mide el arranque con caché, el Resumen en frío y en caché y la contención lectura/escritura; sección «Después de la Fase 2» en `docs/perf/BASELINE.md`; un paso que no mejore se revierte.
- **Documentación**: `README.md`, `CONTRIBUTING.md` y `docs/wiki/` pasan a «solo lectura de los logs; caché local reconstruible».

## Capabilities

### New Capabilities
<!-- Ninguna. -->

### Modified Capabilities
- `almacenamiento-local`: la base deja de ser solo en memoria y pasa a caché reconstruible en disco (o en memoria por ajuste); esquema con reconstrucción ante versión distinta; lecturas por conexiones separadas sin esperas mutuas; recuperación de una caché dañada; caché fiel a los logs presentes; días locales coherentes con la zona horaria.
- `ajustes`: ajuste «Solo memoria» y acción «Reconstruir caché».
- `rendimiento`: escritor único por lotes, coste precalculado con recálculo al cambiar precios, agregados diarios incrementales, caché de resultados, resultados de consultas idénticos, presupuestos de arranque y consultas, estado de lectura persistente y comparación documentada de la fase.

## Impact

- **Código Rust**: `db.rs` (almacenamiento en disco o memoria, PRAGMAs, validación y reconstrucción, `remove_legacy_db`), migración nueva en `migrations/` (columnas precalculadas, disparadores, agregados, índices, metadatos de la caché), módulo nuevo de agregados derivados, `ingest.rs` (lotes por filas/tiempo, día local, comprobación de sesión sin claves foráneas, limpieza de archivos desaparecidos), `ingest_thread.rs` (trabajos de precios y reconstrucción, contador de generación), `queries.rs`/`insights.rs`/`sessions.rs` (lectura de agregados y sin `ROW_NUMBER`), `pricing.rs` (aplicación de precios por diferencias), `commands.rs` (lectores, caché de resultados, comando de reconstrucción), `settings.rs`, `lib.rs`, `alerts.rs`/`tray.rs` (lectores).
- **Frontend**: `src/lib/api.ts`, `src/components/SettingsDialog.tsx`, `src/lib/i18n.ts`, `src/App.tsx`, `src/dev/mock.ts`.
- **Medición y tests**: `examples/perf_scan.rs`, `benches/summary_queries.rs`, `benches/common/mod.rs`, test y golden nuevos de consultas en `src-tauri/tests/`.
- **Servidor MCP**: sin cambios de comportamiento; sigue leyendo los logs a una base en memoria al arrancar (aprovecha los agregados porque usa la misma ingesta).
- **Dependencias**: ninguna nueva; se activa la feature `unlock_notify` de `rusqlite` (ya presente) para el modo «Solo memoria».
- **Datos del usuario**: aparece `~/.local/share/agentboard/cache.sqlite` (más `-wal`/`-shm`) salvo con «Solo memoria».
- **Fuera de alcance**: relecturas dirigidas del vigilante y eventos `data:changed` (Fase 3), rendimiento del frontend (Fase 4) salvo la interfaz mínima de Ajustes de este cambio.
