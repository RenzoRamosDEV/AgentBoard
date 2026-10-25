# Design

## Context

- Motivación: proposal.md (Why). Requisitos: `specs/vigilancia-en-vivo`, `specs/rendimiento`, `specs/dashboard`, `specs/sesiones`, `specs/proyectos`, `specs/interfaz`. Cifras de partida: `docs/perf/BASELINE.md`, «Después de la Fase 2» y «Seguimientos para la Fase 3».
- Estado observado:
  - `watcher.rs`: un `notify::recommended_watcher` con `RecursiveMode::Recursive` por raíz existente; el callback descarta el evento y manda `()`; `recv_timeout(RECHECK = 20 s)` para descubrir raíces nuevas; debounce global de 400 ms (máx. 5 s) y `ingest.request(Origen::Vigilante)`, que hace `scan_with_hooks` completo: recorre todas las raíces (`walk`) y hace un `stat` por archivo.
  - `ingest_thread.rs`: dueño del escritor; `Job::Scan(Origen) | AplicarPrecios | Reconstruir`; agrupa escaneos pendientes; `on_done(origen, stats)` tras cada escaneo; `DataGeneration` sube en cada `COMMIT` (`BatchHooks::committed`).
  - `ingest.rs`: `scan()` = recorrido + `stat` + `forget_stale_files` (limpieza de D10 de la Fase 2: borra las sesiones de un archivo desaparecido, encogido o rotado y reinicia a offset 0 sus archivos relacionados) + plan + lectura paralela + escritura ordenada por lotes. `Provider::matches(path)`; OpenCode es `Source::Db` (`opencode.db`, sus cambios llegan también por `opencode.db-wal`).
  - `lib.rs`: `ingest://done` con `ScanStats` al terminar el escaneo inicial y `()` tras cada relectura del vigilante con registros; en ambos casos `refresh_alerts` (avisos de presupuesto y bandeja).
  - Interfaz: `App.tsx` y `TrayPanel.tsx` escuchan `ingest://done` y suben `refresh`; `useDashboardData` lanza 19–23 `invoke`; `TrayPanel` además refresca cada 30 s aunque esté oculto; `TodayLive` cada 30 s (caducan «activa ahora» y el ritmo de la última hora); `SettingsDialog`/`cache.ts` esperan `ingest://done` para terminar «Reconstruir caché»; `src/dev/mock.ts` simula los comandos y emite `ingest://done`.
  - Listas: `list_sessions(filter, limit ≤ 500)`; `get_project_detail` incluye **todas** las sesiones del proyecto (`MAX_AGGREGATED = 100 000`); `get_session_detail` incluye todos los turnos y un punto de coste por llamada. El servidor MCP usa `sessions::list_sessions`, `project_detail` y `session_detail` directamente.
  - Consultas: `insights::activity` y `activity_daily` llaman cada una a `turn_stats`; `activity_daily` recorre todas las llamadas del filtro en Rust y clasifica el turno por llamada. `queries::source(conn, f, tz)` ya decide ruta de agregados o de detalle.
  - WAL: `journal_size_limit` 8 MiB, `wal_autocheckpoint` por defecto (PASSIVE); con un lector siempre activo el checkpoint no reinicia el WAL.

## Goals / Non-Goals

**Goals:**
- Presupuestos de la tabla de D1, medidos de forma reproducible con `perf_scan`.
- Datos idénticos: golden de ingesta y de consultas sin regrabar; la relectura dirigida deja la base igual que un escaneo completo.
- Cero dependencias nuevas.

**Non-Goals:**
- Rendimiento del frontend (memorización, virtualización, render): Fase 4. Aquí solo se cambia cómo pide los datos.
- Refresco selectivo por `agents`/`days` en la interfaz: el evento los lleva, la interfaz aún refresca todo.
- Cambios visuales.
- Herramientas o salidas nuevas del servidor MCP.
- Aplazar los derivados del escaneo frío (seguimiento de la Fase 2 sin presupuesto en esta fase).

## Decisions

### D1. Medición, perfiles y regla de conservación
- Perfil nuevo `[profile.bench-fast]` (`inherits = "release"`, `lto = false`, `codegen-units = 16`, `incremental = true`) para todos los pasos intermedios; `release` (LTO fat) solo en la medición final, A/B contra el binario de la **partida** (commit de la tarea 1, que ya tiene las fases nuevas de `perf_scan`).
- `perf_scan` (todas con `--cache <ruta>` salvo que se indique):

| Objetivo | Cómo se mide | Presupuesto |
|---|---|---|
| Evento → dato visible | `--phase watch`: escaneo inicial, vigilante + notificador reales; 20 escrituras de una línea nueva en un log del fixture separadas 1,5 s; tiempo desde que `write` vuelve hasta que un snapshot de todo el historial, pedido al recibir `data:changed`, cuenta la llamada nueva. Mediana y p95 | mediana < 500 ms (200 MB) |
| Refrescos por ráfaga | `--phase watch --events 500`: 500 líneas en 50 logs en 3 s; instantes de cada `data:changed`, número de relecturas y de escaneos completos | separación entre avisos ≥ 1 s; 0 escaneos completos |
| CPU en reposo | `--phase idle --idle-secs 60`: tras el escaneo inicial, vigilante y notificador en marcha sin escrituras; CPU del proceso (`utime + stime` de `/proc/self/stat`) y relecturas en ese tiempo | < 60 ms de CPU y 0 relecturas en 60 s |
| Contención en disco | `--phase contention --repeat 200 --reader ui` (lector que pide un snapshot por aviso, como la interfaz); `--reader loop` (Resumen en bucle, el de la Fase 2) se sigue imprimiendo sin presupuesto | `ui` ≤ ×1,1 |
| WAL acotado | La misma fase imprime el tamaño del `-wal` al terminar y el Resumen en frío antes y después de la tanda | WAL ≤ 8 MiB; después ≤ 1,2 × antes |
| `activity_daily` | `--phase queries` sobre `1g` (por consulta, ya existe) | < 300 ms |
| Snapshot del Resumen | `--phase queries` añade `snapshot` (frío y repetido, con comparación) y comprueba que coincide con las consultas sueltas | < 300 ms en frío, 200 MB |
| Respuestas compactas | `--phase queries` imprime los bytes JSON de cada comando de la interfaz con el filtro de 30 días, incluido el detalle de la sesión más grande (`sesion-grande-*`) | ≤ 1 MiB salvo exportar |

- **Regla de conservación**: cada paso de optimización se mide con `bench-fast` (tres ejecuciones, mediana, sobre `200m`; `1g` en `activity_daily`) frente al paso anterior. Se conserva si mejora su métrica ≥ 5 % sin empeorar otra > 5 %, o si es estructural (vigilante, evento, snapshot, paginación) y no empeora > 5 %; si no, se revierte y se anota en «Desviaciones durante la implementación» y en `docs/perf/BASELINE.md`.
- Las fases nuevas solo escriben dentro de la carpeta del fixture (archivos `perf-live-*`/`perf-burst-*`, borrados al terminar), como exige el spec.

### D2. Vigilante: filtro y debounce por archivo
- El callback de `notify` envía el `notify::Event` completo (o el error) por el canal.
- Filtro puro `relevant(event, providers) -> Vec<PathBuf>`: descarta `EventKind::Access(_)`; normaliza las rutas de bases SQLite quitando `-wal`/`-shm`/`-journal`; conserva una ruta solo si algún proveedor la reconoce (`matches`, en el orden de proveedores); para `Create` de una carpeta dentro de una raíz, devuelve los archivos reconocidos que ya contenga (`walk` de esa carpeta, solo de ella: pudieron escribirse antes de que `inotify` añadiera su vigilancia). `Remove` y `Rename` se conservan (la relectura dirigida decide qué hacer con una ruta que ya no existe).
- `Debouncer` puro, con el reloj como parámetro: `HashMap<PathBuf, (primer evento, último evento)>`; un archivo está listo cuando lleva `QUIET = 200 ms` sin eventos o cuando han pasado `MAX_WAIT = 1 s` desde su primer evento pendiente (un log que no para de crecer se relee al menos una vez por segundo). `next_deadline()` da el instante más próximo; `take_ready(now)` devuelve los listos, ordenados.
- Bucle: con pendientes, `recv_timeout(next_deadline − now)`; sin pendientes, `recv()` bloqueante. Al vencer, `ingest.rescan(listos)`. Sin intervalos fijos.
- `event.need_rescan()` (cola del SO desbordada) o un error de `notify` → `ingest.request(Origen::Vigilante)` (escaneo completo), como hoy.
- **Alternativas**: `notify-debouncer-full` (al vigilar construye un mapa de identidades recorriendo todo el árbol, coste innecesario sobre 1 GB, y aun así harían falta el filtro por proveedor y la espera máxima); mantener el debounce global (no sabe qué archivos releer).

### D3. Carpetas nuevas sin sondeo
- Por cada raíz de cada proveedor: si existe, vigilancia recursiva; si no, vigilancia **no recursiva** del antepasado existente más profundo (p. ej. `~` para `~/.codex/sessions`). Un `Create`/`Rename` en un antepasado vigilado que esté en el camino de una raíz pendiente reevalúa esa raíz: vigila el siguiente antepasado o, si ya existe, la raíz entera, y pide una relectura dirigida de todos sus archivos reconocidos (los escritos antes de vigilarla).
- Si `watch()` falla en una raíz que existe (p. ej. límite `max_user_watches`), se avisa con `tracing::warn!` y **solo para esa raíz** se mantiene un rechequeo de 30 s que la relee entera: intervalo justificado (sin él esa raíz no se actualizaría nunca) y de coste nulo cuando todas las raíces se vigilan. Es el único temporizador del vigilante.
- **Alternativas**: el `RECHECK` de 20 s actual (despierta siempre); vigilar `~` de forma recursiva (miles de vigilancias inútiles).

### D4. Relectura dirigida en el hilo de ingesta
- `IngestHandle::rescan(paths)` → `Job::Rescan(BTreeSet<PathBuf>)`. Agrupación en `run`: varios `Rescan` pendientes se unen; un `Scan` completo pendiente los absorbe; precios y reconstrucciones conservan su prioridad actual.
- `ingest::scan_paths(conn, providers, paths, hooks)`: misma tubería que `scan` (lectura paralela, escritura ordenada por lotes, `BatchHooks`) pero el plan sale de las rutas dadas en lugar de `walk`: cada ruta se asigna al primer proveedor que la reconoce, se ordena como el escaneo completo (proveedor, raíz, ruta) y se hace `stat` solo de ellas. La limpieza de D10 de la Fase 2 se aplica a esas rutas (desaparecida, encogida o rotada); los archivos relacionados que reinicia a offset 0 se añaden al plan de la misma pasada, para que la base quede como tras un escaneo completo. Se refactoriza `scan` en «plan» + «ejecución» para compartir el código.
- El origen de un `Rescan` es `Origen::Vigilante`. `ScanStats` añade `full: bool` (para comprobar «0 escaneos completos» en la ráfaga).
- **Alternativa**: seguir con el escaneo completo y confiar en `is_unchanged` (hoy 20 ms con 200 MB y 98 ms con 1 GB solo en `stat`/recorrido, por cada relectura).

### D5. Evento `data:changed` con throttle
- `derived::refresh` devuelve `Touched { agents: BTreeSet<String>, days: BTreeSet<i64> }` leído de `dirty_keys ⨝ sessions` antes de vaciarla (días `YYYYMMDD` en la zona de la caché). `BatchHooks::committed(conn, &Touched)`. Precios, limpieza y reconstrucción marcan `Touched { all: true }`, que en el evento viaja como listas vacías: «puede haber cambiado todo» (documentado en el tipo de `api.ts`).
- Módulo nuevo `notifier.rs`: `Throttle` puro con reloj inyectado: `on_change(now, generation, touched) -> Emit(payload) | Pending(deadline)` y `on_deadline(now) -> Option<payload>`. Primer cambio tras ≥ 1 s sin avisos: sale al momento; dentro del segundo: se acumula (unión de agentes y días, última generación) y sale al cumplirse el segundo desde el aviso anterior. Un hilo `notifier` lo atiende con `recv()` sin pendientes y `recv_timeout` con pendiente. Al emitir: `app.emit("data:changed", payload)` y `refresh_alerts()` (bandeja y avisos, ahora ≤ 1/s).
- Un `COMMIT` sin cambios (`Touched` sin agentes ni días y sin `all`) no emite: la relectura que no encuentra nada nuevo no despierta a la interfaz.
- **Compatibilidad de `ingest://done`**: se conserva solo para el fin del escaneo inicial y del de una reconstrucción (con `ScanStats`), porque «Reconstruir caché» espera ese fin, no un cambio de datos. Todos los oyentes que refrescan datos (`App`, `TrayPanel`) migran a `data:changed`; el vigilante deja de emitir `ingest://done`. El indicador «hace N s» se actualiza con `data:changed` y su resalte solo tras el primer `ingest://done`.
- **Alternativas**: throttle en la interfaz (el backend seguiría emitiendo y refrescando bandeja y avisos por lote); seguir emitiendo `ingest://done` para todo (mezcla «terminó un escaneo» y «cambiaron datos»; la reconstrucción terminaría con el primer lote).

### D6. Snapshot del Resumen
- Módulo nuevo `snapshot.rs` (dominio, sin Tauri): `DashboardRequest { filter, tz, month_from, branch_view, previous: Option<Filter> }` → `DashboardSnapshot` con los mismos campos y tipos que hoy recibe `useDashboardData` (`summary`, `agents`, `daily`, `hourly`, `daily_by_*`, `month`, `projects`, `branches`, `models`, `activity`, `activity_daily`, `tools`, `commands`, `skills`, `mcp`, `agent_types`, `prev`). `month_from` lo calcula la interfaz como hoy (`monthStart()` local).
- Se ejecuta en **una** transacción de lectura (`BEGIN … COMMIT` en el lector): todos los apartados ven el mismo estado y la lectura dura lo mismo que las consultas, sin huecos entre `invoke`. `turn_stats` se calcula una vez y lo comparten `activity` y `activity_daily` (`insights::activity_from`/`activity_daily_from`).
- Comando `get_dashboard_snapshot(request)` con la caché de resultados (`key_at_minute("snapshot", request, now)`, porque `summary` depende del minuto). Los comandos sueltos se quedan: los usan la barra lateral, «Hoy en directo», la bandeja, Precios y el MCP.
- Interfaz: `useDashboardData` hace un único `invoke` y añade `filter` y `prev.filter` en el cliente; `mock.ts` compone el snapshot con sus funciones existentes.
- **Alternativas**: agrupar los `invoke` en el cliente (sigue habiendo 20 viajes y lecturas separadas); un snapshot con tipos nuevos compactos (cambiaría todos los paneles: Fase 4).

### D7. `activity_daily` en 1 GB
- Paso A (siempre): cuando `queries::source(conn, f, Some(tz))` elige agregados (zona de la caché y días completos), `activity_daily` agrupa en SQL con la columna `calls.day`: `GROUP BY turn_id, day` para llamadas con turno y `GROUP BY day, activity` para las que no lo tienen (`SUM(cost_usd)`, `MIN(ts)`), convierte cada `day` a inicio de día local una vez (memo) y clasifica cada turno una vez (memo `turn_id → actividad`). Fuera de esa ruta, el recorrido actual con el memo de clasificación. Mismo resultado: golden de consultas sin regrabar.
- Paso B (solo si tras A sigue ≥ 300 ms en `1g`): tabla derivada `turn_days(turn_id, day, cost_usd, first_ts)` mantenida por `derived::refresh` para las sesiones sucias (migración nueva; una caché de otra versión se reconstruye, D2 de la Fase 2). Se mide con la regla de D1.
- **Alternativa**: precalcular la actividad del turno en la ingesta (depende de herramientas que llegan después del turno; invalida más).

### D8. WAL: checkpoint con el escritor libre
- Al terminar un trabajo `Scan`/`Rescan` y con la cola vacía, en caché en disco: si el `-wal` supera `JOURNAL_SIZE_LIMIT` (8 MiB), `PRAGMA wal_checkpoint(TRUNCATE)` con `busy_timeout` de 100 ms solo para esa sentencia; si devuelve ocupado, se reintenta tras el siguiente trabajo. En memoria no aplica. Las lecturas agrupadas de D5/D6 (un snapshot corto por aviso) dejan huecos sin lectores para que el checkpoint complete.
- **Alternativas**: `RESTART` por lote (bloquea al escritor en cada lote si hay lector); bajar `wal_autocheckpoint` (no reinicia el WAL con un lector activo).

### D9. Paginación y tipos compactos
- `sessions.rs`: `Page<T> { rows, total, offset }`; `PageRequest { limit (por defecto 50, máx. 500), offset, sort: SessionSort, desc }`; `SessionSort` es un `enum` de serde con las columnas ordenables de hoy (`started`, `agent`, `branch`, `model`, `duration`, `turns`, `compactions`, `cost`): un valor desconocido falla al deserializar (error controlado) y la columna SQL sale de un `match`, nunca del texto recibido. Orden estable con `s.id` como desempate.
- `session_page(conn, filter, project_key, page)` reutiliza `query_sessions` con `ORDER BY`/`LIMIT`/`OFFSET` y `COUNT` para el total.
- `project_overview(conn, key, filter, tz)` = detalle actual sin `sessions`, más `sessions_with_compactions`. `session_overview(conn, id)` = detalle sin `turns`, con `timeline` en columnas `{ ts: [i64], cost: [f64] }`, `turns_total` y `after_compaction: Option<{ k, ratio }>` (la lógica de `afterCompaction` de `SessionDetail.tsx` pasa a Rust, con su test). `session_turns(conn, id, page)` devuelve `Page<TurnRow>` en orden.
- El MCP no cambia: `project_detail` y `session_detail` siguen existiendo como composición de lo anterior (salida idéntica, cubierta por sus tests).
- Comandos: `list_sessions(filter, page, projectKey?)`, `get_project_detail` → overview, `get_session_detail` → overview, nuevo `list_session_turns(id, page)`. `list_project_summaries` no se pagina: crece con el número de proyectos, no con el historial.
- Interfaz: `DataTable` admite modo controlado (`sort`/`onSortChange` sin ordenar en cliente, y `more: { total, onMore }` para «Ver más» que añade la página siguiente). `ProjectDetail` (sesiones, 25 por página) y `SessionDetail` (turnos, 30 por página) lo usan; `TodayLive` pide `limit 200` y `CommandPalette` `limit 30` como hoy. Un `data:changed` recarga desde la primera página.
- **Alternativas**: paginar por cursor (no encaja con orden por cualquier columna); paginar en el cliente (el payload seguiría trayendo todo).

### D10. Reposo en la interfaz
- `TrayPanel` deja su intervalo de 30 s: ya refresca con `panel://shown` y `data:changed`.
- `TodayLive` conserva su tick de 30 s (la tarjeta caduca con el reloj: «activa ahora», última hora, medianoche) pero lo omite con `document.visibilityState === "hidden"`. Coste: 4 consultas de milisegundos cada 30 s con la ventana visible.

### D11. Documentación
- `docs/wiki/Arquitectura.md`: vigilante (debounce por archivo, relectura dirigida, sin sondeo), eventos `data:changed`/`ingest://done`, snapshot y paginación. `docs/perf/BASELINE.md`: sección «Después de la Fase 3» y fases nuevas en «Reproducción».

## Risks / Trade-offs

- [La relectura dirigida diverge del escaneo completo en casos raros (rotación, archivo relacionado de otra raíz)] → test de propiedad con secuencias de escrituras, borrados, rotaciones y truncados sobre el fixture `2m`: tras cada relectura dirigida, huella igual a un escaneo completo en base nueva; el escaneo completo sigue en el arranque.
- [Eventos perdidos (desbordamiento, `max_user_watches`)] → `need_rescan` dispara escaneo completo; raíz no vigilable con rechequeo de 30 s (D3).
- [Latencia bajo ráfaga: hasta 1 s de espera máxima del debounce más 1 s de throttle] → el presupuesto < 500 ms se define para una escritura aislada (spec); bajo ráfaga manda «≤ 1 aviso/s».
- [Paginación por desplazamiento con datos que cambian: «Ver más» puede repetir u omitir una fila] → cada `data:changed` recarga desde la primera página; aceptable para tablas de consulta.
- [El snapshot en una transacción alarga la lectura más larga] → sigue siendo < 300 ms; el WAL se reinicia entre snapshots (D8).
- [`TRUNCATE` bloquea al escritor hasta 100 ms] → solo con el escritor libre y el WAL por encima del límite.
- [`days` en la zona de la caché, no en la de la interfaz] → la interfaz aún no los usa (Non-Goals); documentado en el tipo.
- [Cambio de firmas de comandos internos] → interfaz y `mock.ts` en el mismo paso; MCP intacto.

## Migration Plan

- Sin cambio de esquema salvo el paso B de D7 (migración nueva: la caché de otra versión se reconstruye sola al arrancar).
- Despliegue: binario nuevo; la interfaz y los comandos van juntos. Reversión: revertir los commits de la fase; la caché se reconstruye si hubo migración.

## Desviaciones durante la implementación

### Partida

Commit de partida `97088e3` (fases nuevas de `perf_scan`, sin cambios de comportamiento). `bench-fast`,
`200m` con `--cache` nueva por ejecución, tres ejecuciones (mediana; rango entre paréntesis), load 0,4–2,3.

| Medida | Partida |
|---|---|
| `watch`: evento → dato visible (mediana / p95) | 800 ms (799–801) / 807–818 ms; 20 escaneos completos en 20 escrituras |
| `watch --events 500`: avisos / relecturas / escaneos completos | 1 / 1 / 1 (el debounce global espera a que acabe la ráfaga); separación n/d |
| `idle --idle-secs 60`: CPU / relecturas | 0 ms (0–10) / 0 |
| `contention --repeat 200 --reader ui` | × 1,38 (41,6 → 57,0 ms); WAL 1 254,9 MiB; Resumen en frío antes / después 143 / 2 430 ms (× 16–17) |
| `contention --reader loop` | × 1,38 (42,1 → 57,8 ms); WAL 1 259,3 MiB; × 15,8–16 |
| `queries` 200m: Resumen completo frío / repetido | 133 ms (132–147) / 0,0 ms |
| `queries` 1g: Resumen completo frío; `activity_daily` | 781 ms (778–794); 255 ms (255–256) |
| Respuesta mayor (bytes, 200m, 30 días) | `get_session_detail` de la mayor: 126,5 KiB; `get_timeseries(hour)` 102,3 KiB; `list_sessions` 88,5 KiB |

Las consultas de `200m` están en el rango de «Después de la Fase 2» (171 ms en `release` con load
1,5–3,5; aquí 133 ms con menos carga). `activity_daily` en `1g` ya baja de 300 ms en `bench-fast` con
poca carga (255 ms frente a 341 ms en `release` con load 1,5–3,5): el paso A de D7 se hace igual (lo
exige la tarea) y el paso B solo si sigue ≥ 300 ms.

Notas de la herramienta:
- «Resumen en frío antes / después» de `contention` se mide tras la tanda sin lecturas y tras la tanda
  con lecturas (cada tanda añade 200 × 1000 llamadas a la sesión en vivo, así que parte de la subida
  es crecimiento de datos y no WAL).
- En la partida, el aviso a la interfaz de `watch` es `on_done(Vigilante)` con registros (lo que
  emitía `ingest://done`) y todos los escaneos son completos.

### Paso 2–3: relectura dirigida y vigilante (D2–D4)

`bench-fast`, `200m`, tres ejecuciones (load 0,2–0,9). Estructural: **se conserva**.

| Medida | Partida | Paso 3 |
|---|---|---|
| `watch`: evento → dato visible (mediana / p95) | 800 / 812 ms | **569 ms** (568–570) / 577–583 ms; 0 escaneos completos |
| `watch --events 500`: avisos / relecturas / escaneos completos | 1 / 1 / 1 | 500 / 500 / **0** (cada log recibe una línea cada 300 ms > `QUIET`: cada línea es una relectura; el throttle llega en el paso 4) |
| `idle --idle-secs 60`: CPU / relecturas | 0 ms / 0 | 0 ms / 0 (sin el `RECHECK` de 20 s) |

- La latencia restante es sobre todo la lectura que hace la medición al recibir el aviso: el Resumen
  de todo el historial de `200m` (≈ 300 ms en frío tras cada cambio), más los 200 ms de `QUIET`.
- `perf_scan` pasa a usar la ruta absoluta de los fixtures (`canonicalize`): el vigilante filtra por
  raíz y `notify` entrega rutas absolutas. La partida no filtraba, así que sus cifras siguen valiendo.
- Test de propiedad (`tests/relectura_dirigida.rs`, en `tests/` porque usa `common::fingerprint`):
  la relectura dirigida deja **siempre** la misma huella que el escaneo completo incremental (24 de
  24 pasos). Frente a leer desde cero coinciden 22 de 24: en dos pasos forzados (borrar el log
  principal de una sesión cuyos subagentes y continuaciones están en otros archivos ya truncados o
  rotados) el propio escaneo completo incremental ya difiere de leer desde cero (turnos de otra
  sesión). Es un límite previo de D10 de la Fase 2, no de la relectura dirigida: el test lo acota
  (≥ 20 de 24) y queda como seguimiento. Las líneas que el test añade a una sesión existente
  repiten su `cwd`, rama y modelo guardados: con valores distintos repartidos entre archivos, esos
  campos dependen del orden de lectura también en el escaneo completo incremental.

### Paso 4: `data:changed` con throttle (D5)

`bench-fast`, `200m`, tres ejecuciones (load 1,5–8,5: carga ajena en la máquina). Estructural:
**se conserva**.

| Medida | Paso 3 | Paso 4 |
|---|---|---|
| `watch`: evento → dato visible (mediana / p95) | 569 ms | 574 ms (571–577) / 579–595 ms (+1 %, dentro del ruido con más carga) |
| `watch --events 500`: avisos / relecturas / escaneos completos | 500 / 500 / 0 | **4** / 500 / 0 |
| Separación mínima entre avisos | 3,5 ms | **1 000,1 ms** |

- `perf_scan --phase watch` escucha ahora el notificador real (`data:changed`) en lugar de
  `on_done`.
- Un `COMMIT` sin cambios (p. ej. media línea) sube la generación pero no avisa.
- La limpieza de D10 (log borrado, rotado o truncado) confirma su propia transacción fuera de los
  lotes: ahora avisa con `Touched::everything()` (antes no subía la generación; un log borrado no
  habría refrescado la interfaz sin un lote posterior).
- Los días de las llamadas sumadas por `Deltas` no pasan por `dirty_keys`: se anotan en una tabla
  temporal del escritor (`temp.touched_days`) que `derived::refresh` lee y vacía.
- Tarea 4.4: la comprobación manual en `npm run tauri dev` no se hizo (sesión sin interacción
  gráfica); la cubren `perf_scan --phase watch` (vigilante, ingesta y notificador reales) y los
  tests de suscripción del frontend.

### Paso 5: snapshot del Resumen (D6)

`bench-fast`, tres ejecuciones (load 1,5–3,1). Estructural: **se conserva**.

| Medida | Antes | Paso 5 |
|---|---|---|
| Resumen 200m en frío: 18 consultas sueltas / snapshot **con comparación** | 120–128 ms / — | **106 ms** (106–120) |
| Resumen 1g en frío: sueltas / snapshot con comparación | 738–755 ms / — | 626 ms (622–628) |
| Snapshot repetido (caché de resultados) | — | 0,0 ms |
| `watch`: evento → dato visible (la medición ya lee con el snapshot) | 574 ms | **515 ms** (515–515) / p95 521–527 ms |
| Respuesta mayor (bytes, 200m) | 126,5 KiB | 246,1 KiB (`get_dashboard_snapshot`, que sustituye a 20 respuestas) |

- `turn_stats` se comparte entre `activity` y `activity_daily` (`insights::activity_from` /
  `activity_daily_from`).
- `perf_scan --phase queries` no compara el snapshot con las consultas sueltas: lo hace el test
  `tests/snapshot_resumen.rs` (fixtures `2m` y versionados, en memoria y en disco, con y sin
  comparación y vista por ramas) con el comparador del golden.
- Comprobado en `npm run dev` (mock) con Chrome sin cabeza (`?probe=1`): el Resumen se pinta con
  las cifras del periodo y del anterior y la sonda no marca desbordes.

### Paso 6: `activity_daily` en 1 GB (D7) — **revertido**

Paso A implementado (`9907cb2`: dos `GROUP BY` sobre `calls` por turno/día/actividad y por
día/actividad, memo de clasificación y de medianoche local) con sus tests (igual que el detalle en
las particiones del filtro, SQL agrupado, cada turno clasificado una vez). Medido con `bench-fast`,
tres ejecuciones, frente a la partida:

| Medida | Antes | Paso A |
|---|---|---|
| `activity_daily` 1g (mediana) | 241 ms (240–242; partida 255) | 263 ms (262–270) (+9 %) |
| `activity_daily` 200m (mediana) | 37–39 ms | 45–47 ms (+18 %) |

Empeora: ordenar todas las llamadas del periodo para agruparlas por turno cuesta más que
recorrerlas, y lo caro sigue siendo `turn_stats`. Por la regla de D1 se **revierte** (`b412bab`),
tests incluidos. El paso B (tabla `turn_days`) **no se hace**: `activity_daily` en `1g` ya está por
debajo de 300 ms sin el paso A (241–255 ms en `bench-fast`; se confirma en `release` en la medición
final). El tamaño en bytes de la respuesta cambiaba (20,1 → 19,4 KiB en `200m`) solo por la
representación decimal de sumas hechas en otro orden (golden de consultas en verde).

### Paso 7: WAL acotado (D8)

`bench-fast`, `200m`, `--repeat 200 --cache`, tres ejecuciones (load 1,9–5,8). La fase `contention`
reproduce ahora al escritor de la app: tras cada relectura, con el escritor libre,
`checkpoint_if_large`; y el lector `ui` recibe sus avisos por el notificador real (como mucho uno
por segundo) y lee el snapshot del Resumen. Se imprimen los checkpoints hechos y aplazados.

| Medida | Partida | Paso 7 |
|---|---|---|
| `--reader ui`: con / sin lecturas | × 1,37–1,38 | **× 1,22–1,32** |
| `--reader ui`: WAL al terminar | 1 254,9 MiB | 1 254,9 MiB (0 checkpoints hechos, 198 aplazados) |
| `--reader loop`: con / sin lecturas | × 1,37–1,40 | × 1,34–1,35 |
| `--reader loop`: WAL al terminar | 1 259,3 MiB | **236–463 MiB** (4–7 checkpoints hechos) |
| Resumen en frío después / antes (ui) | × 2,04–2,06 (partida recompilada) | × 1,23–2,18 |

Estructural y mejora: **se conserva**. Presupuestos de D1 (`ui` ≤ × 1,1, WAL ≤ 8 MiB, después ≤ 1,2
× antes) **no se cumplen** en esta carga:

- Cada tanda añade 200 000 llamadas a una única sesión del último día, dentro del filtro de 30
  días: el Resumen de esa sesión pasa de ~140 ms a 1,1–2,4 s. Con lecturas de ~2 s y un aviso por
  segundo siempre hay un lector abierto: el `TRUNCATE` no puede completar nunca (aplazado 198 de
  198 veces) y el escritor no se bloquea más de 100 ms por intento. Con el lector en bucle, que deja
  huecos entre lecturas, sí completa y el WAL final baja de 1,25 GiB a 0,24–0,46 GiB.
- «Resumen en frío después / antes» compara el Resumen con 200 000 y con 400 000 llamadas nuevas:
  mide sobre todo el crecimiento de los datos, no el WAL.
- **Corrección de la partida**: el «antes» de 143 ms de la partida se midió con un binario anterior
  a mover esa medida tras la tanda sin lecturas (el binario no se recompiló). Recompilado en
  `97088e3`, la partida da 1 171–1 197 ms antes / 2 393–2 462 ms después (× 2,04–2,06).

### Paso 8: páginas y tipos compactos (D9)

Bytes JSON (`200m`, filtro de 30 días): ninguna respuesta pasa de 1 MiB (la mayor, el snapshot del
Resumen, 246,1 KiB). `get_session_detail` de la sesión más grande 126,5 → **51,9 KiB** (sin turnos,
línea en columnas; sus turnos van en páginas de 30: 5,6 KiB); `get_project_detail` del proyecto
mayor 10,7 → 5,5 KiB (sin sesiones); `list_sessions` 88,5 KiB (hasta 500) → 23,3 KiB (página de 50).

- El servidor MCP no cambia: `list_sessions`, `project_detail` y `session_detail` siguen igual
  (golden nuevo `tests/golden/detalle_mcp_fixtures.txt`, grabado en la partida de `sessions.rs`
  antes de tocarlo, y los tests de `agentboard-mcp`).
- `project_overview` y `session_overview` se componen sobre `project_detail` y `session_detail`
  (salida idéntica a la del MCP por construcción); `session_turns` pagina la lista completa de
  turnos de la sesión (el cálculo por turno necesita todas sus llamadas).
- Interfaz: `DataTable` admite orden controlado (`sort`/`onSortChange`) y «Cargar más» (`more`); el
  paginado lo lleva `lib/pager.ts` (descarta respuestas de cargas anteriores). En el turno no se
  ordena (llega en orden). Texto nuevo «Cargar más ({n} de {total})» en es/en/pt/fr. Comprobado
  con Chrome sin cabeza sobre el mock: detalle de proyecto (página, total, sesiones con
  compactaciones) y de sesión (marcas de compactación y aviso calculado en el backend).

### Mutación dirigida (tarea 9.1)

`cargo mutants --in-place --in-diff <diff desde 97088e3> --cargo-arg=--lib -- <módulo>`, cada
ejecución cortada a 3 min con `SIGINT`, árbol limpio después (`git status`). Con los tests del módulo
cada mutante tarda 3–5 s, así que casi todas las ejecuciones terminaron solas.

| Alcance | Examinados | Atrapados | Sin compilar | Tiempo agotado | Supervivientes |
|---|---|---|---|---|---|
| `notifier.rs` (`Throttle`) | 20 | 14 | 6 | 0 | 0 |
| `watcher.rs` (`normalize`, `under_root`, `recognized`, `relevant`, `Debouncer`, `watch_plan`) | 25 (+10 al repetir) | 17 (+9) | 5 (+1) | 0 | 3 → 1 |
| `sessions.rs` (orden, `PageRequest`, `session_page`, `project_overview`, `after_compaction`, `session_turns`) | 34 (+14) | 20 (+13) | 12 (+1) | 0 | 2 → 0 |
| `snapshot.rs` | 5 (+5) | 0 (+2) | 3 (+3) | 0 | 2 → 0 |
| `ingest.rs` (planificación de `scan_paths`: `scan_given`, `plan_paths`, `load_tracked_paths`, `forget_stale_files`, avisos) | 18 (+5) | 15 (+5) | 1 | 1 | 1 → 0 |

Los supervivientes se mataron con tests nuevos (evento `Modify` sobre una carpeta no relee su
contenido; turno justo en la compactación y razón distinta de 1; mes del snapshot con su inicio y
sin el final del periodo; archivo relacionado que tampoco existe) y se repitió la mutación del
alcance afectado. Queda **1 superviviente justificado**: `under_root → true` en `watcher.rs` es
equivalente en resultado (`recognized` vuelve a exigir la raíz); solo evita recorrer carpetas
creadas fuera de las raíces (p. ej. bajo el `~` vigilado sin recursión), un coste que los tests no
observan. Puntuación sobre los mutantes viables examinados: 1 superviviente de 75 (98,7 %). El
mutante con tiempo agotado (`Sink::committed → Ok(())`) es detectado: sin el aviso el test espera el
cambio hasta el límite.
