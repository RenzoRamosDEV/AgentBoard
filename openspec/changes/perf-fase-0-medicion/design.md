# Design

## Context

- Motivación: ver proposal.md (Why). Requisitos: ver `specs/rendimiento/spec.md`.
- Backend Tauri 2 en `src-tauri/` (workspace con el miembro `mcp`, que reutiliza `agentboard_lib`). Base SQLite en memoria.
- Flujo observado:
  - `lib.rs::run` → hilo de escaneo inicial → `ingest::scan_all` → emite `ingest://done`.
  - `watcher::spawn` → debounce → `scan_all` completo con las mismas instancias de proveedor → callback que emite `ingest://done`. El refresco en vivo es, por tanto, un `scan_all` incremental (stat de todos los archivos + lectura de lo nuevo).
  - `scan_all` → `register_agents` + `DELETE` de proyectos huérfanos → por proveedor/raíz: `walk` → `ingest_file` (lectura desde offset, `parse_line` por línea, una transacción por archivo).
- El Resumen (frontend `useDashboardData`) invoca por filtro: `summary`, `breakdown` ×8 (agent, project, model, tool, command, skill, mcp, agent_type; `branch` si hay un proyecto), `timeseries` ×3 (day, hour, mes), `timeseries_by` ×4, `activity`, `activity_daily`, y opcionalmente el periodo anterior.
- Hoy los errores salen con `eprintln!` en `ingest.rs`, `lib.rs` (2), `watcher.rs` (2), `tray.rs` y `mcp/src/main.rs`.
- Entorno: herramientas de compilación en distrobox `agentburn-dev`; `perf` y `cargo-flamegraph` no instalados; 12 cores. `src-tauri/target/` ya está en `.gitignore`.

## Goals / Non-Goals

**Goals:**
- Instrumentación sin coste apreciable con las trazas desactivadas.
- Medición reproducible y aislada (fixtures deterministas, raíces inyectadas).
- Línea base y ranking de cuellos de botella que guíen las fases 1–4.

**Non-Goals:**
- Optimizar ingesta, SQLite, vigilante o UI; corregir los cuellos de botella encontrados.
- Instrumentar el frontend o medir el render de React.
- Spans por línea (`parse_line`) o por registro: su coste distorsionaría la medición; el parseo se mide con criterion.
- Exportadores (OpenTelemetry, archivos de log, JSON).
- Integrar los benchmarks en CI.

## Decisions

### D1. `tracing` + `tracing-subscriber` (fmt + `EnvFilter`), inicializado en un módulo propio
- Módulo nuevo `src-tauri/src/telemetry.rs` (infraestructura, fuera de la lógica de dominio):
  - `filter_from(env: Option<&str>) -> EnvFilter`: `None` → `warn`; valor inválido → `warn` (se avisa una vez por stderr tras inicializar).
  - `subscriber_with_writer(filter, writer)`: fmt a stderr, `with_span_events(FmtSpan::CLOSE)` (imprime `time.busy`/`time.idle` al cerrar), sin ANSI cuando no es TTY.
  - `init()`: lee `AGENTBOARD_LOG` y usa `try_init` (idempotente; no falla si ya hay subscriber, útil en tests y en el MCP).
- Se llama al principio de `lib.rs::run`, en `mcp/src/main.rs` y en `examples/perf_scan.rs`.
- **Por qué**: estándar de facto en Rust, spans con duración nativos, `EnvFilter` cubre la sintaxis pedida y el disparo deshabilitado es una comprobación de nivel cacheada por callsite.
- **Alternativas**: `log` + `env_logger` (sin spans ni duración); cronómetros a mano con `Instant` (ensucia el dominio, sin filtrado); `tracing-chrome`/`tracing-flame` (útiles después, no necesarios para la línea base).

### D2. Nivel por defecto `warn` (no `off`)
- Sin `AGENTBOARD_LOG` no se emiten spans (todos son `info`/`debug`), pero `warn!`/`error!` siguen en stderr. Así se cumplen a la vez "sin la variable no se emite nada" (para trazas) y "los errores siguen saliendo por stderr".
- Mapeo: errores que abortan una operación (escaneo inicial fallido, vigilante detenido, MCP sin historial) → `error!`; errores recuperables (archivo ilegible, carpeta no vigilable, precios del usuario, menú de bandeja) → `warn!`. Se conserva el texto en español y la ruta como campo estructurado.

### D3. Ubicación y nivel de los spans
| Punto | Nivel | Campos |
|---|---|---|
| `scan_all` | `info` | archivos, líneas, registros, errores (registrados al final) |
| `walk` | `debug` | raíz, nº de archivos |
| `ingest_file` | `debug` | ruta, bytes leídos, líneas, registros |
| transacción + commit de `ingest_file` | `debug` (`ingest.tx`) | registros |
| cada `#[tauri::command]` de `commands.rs` | `info` (`cmd.<nombre>`) | — |
| consultas de `queries.rs`/`sessions.rs`/`insights.rs`/`findings.rs` llamadas por comandos | `debug` | `by`/`bucket` cuando aplique |
| emisión de `ingest://done` (escaneo inicial y vigilante) | `debug` (`emit.ingest_done`) | origen |
- Atributo `#[tracing::instrument(level = .., skip_all, fields(..))]` y `Span::current().record(..)` para métricas conocidas al final. `skip_all` evita formatear conexiones/filtros.
- `FileResult` ya cuenta líneas y registros; los bytes son `buf.len()`. No se añaden contadores nuevos al dominio.

### D4. Generador `scripts/gen-fixtures.py`
- Python 3 stdlib (`argparse`, `random.Random(seed)`, `json`, `pathlib`). Uso: `gen-fixtures.py <tamaño> [--seed N] [--out DIR]`; tamaño `50m|200m|1g` o cualquier `<n>{k,m,g}` (los tamaños pequeños sirven para tests).
- Salida por defecto `src-tauri/target/perf-fixtures/<tamaño>/` (ya ignorado por git; evita tocar `.gitignore`). Estructura por agente idéntica a `tests/fixtures/<agente>/` para pasarla como raíz a cada proveedor:
  `claude_code/`, `codex/sessions/`, `copilot/session-state/`, `cursor/projects/`, `gemini/tmp/`.
- Plantillas de línea copiadas estructuralmente de los fixtures versionados (mismos campos/tipos); contenido de mensajes = relleno sintético (palabras de un vocabulario fijo) con longitudes realistas.
- Reparto del tamaño: Claude Code 60 %, Codex 15 %, Copilot 10 %, Cursor 10 %, Gemini 5 %. Varias sesiones, proyectos (≈20), modelos por agente, tool uses (Bash, Read, Edit, Write, Grep, Task, Skill, MCP) y subagentes.
- Siempre incluye una sesión de Claude Code de ~5 MB (`sesion-grande-*.jsonl`) para el bench de archivo completo.
- Determinismo: sin `time.time()`, sin orden de `dict`/`set` no determinista, `sort_keys` no necesario pero orden fijo de campos, timestamps desde una fecha base fija (90 días hacia atrás desde 2026-09-01), UUIDs derivados del RNG. El generador para cuando alcanza el objetivo con margen de ±5 %.
- Test: `scripts/test_gen_fixtures.py` (`unittest`) con tamaño pequeño (p. ej. `2m`): byte a byte igual en dos ejecuciones, tolerancia ±5 %, semillas distintas → contenido distinto, los cinco agentes presentes, todas las líneas JSON válidas.
- **Alternativa**: generador en Rust (`examples/`) — descartado: el requisito pide Python sin dependencias y así no se compila nada para generar 1 GB.

### D5. Benchmarks criterion
- `criterion` (dev-dependency, `default-features = false` + `cargo_bench_support` si hace falta para no arrastrar `plotters`), `[[bench]] harness = false`:
  - `benches/parse_line.rs`: una línea representativa (assistant con `usage` y tool use) por proveedor JSONL (Claude Code, Codex, Copilot, Cursor, Gemini), tomada de `tests/fixtures`.
  - `benches/ingest_file.rs`: ingesta de la sesión de ~5 MB en base en memoria nueva por iteración (`iter_batched`), `Throughput::Bytes`.
  - `benches/summary_queries.rs`: las 5 consultas más usadas del Resumen sobre la base cargada con el fixture `50m`: `summary`, `breakdown(project)`, `breakdown(model)`, `timeseries(day)`, `insights::activity`. Filtro con `from/to` explícitos cubriendo los últimos 30 días de la ventana del fixture (no depende de "ahora").
- Fixture ausente: el bench busca `src-tauri/target/perf-fixtures/50m/`; si no existe intenta `python3 ../scripts/gen-fixtures.py 50m`; si falla, imprime un mensaje claro con el comando y no ejecuta ese grupo (sale con éxito).
- Código común (carga de fixture, construcción de proveedores con `with_roots`) en `benches/common/mod.rs`, reutilizado por `perf_scan`. Si duplicarlo resulta más simple que compartirlo entre `benches/` y `examples/`, se acepta un `#[path]` común.

### D6. Binario `examples/perf_scan.rs`
- Args: `--fixtures <dir>` (obligatorio), `--phase cold|live|queries|all` (por defecto `all`), `--live-lines N` (por defecto 1000), `--repeat R` (por defecto 1).
- Construye cada proveedor JSONL con `with_roots` apuntando a la subcarpeta del fixture; nunca usa `providers::all()` (spec: medición aislada).
- Fases:
  - **cold**: `db::open_in_memory` + `scan_all`; imprime tiempo y `ScanStats`.
  - **live**: crea `claude_code/<proyecto>/perf-live-<pid>.jsonl` con N líneas sintéticas (ids nuevos) y ejecuta `scan_all` con las mismas instancias (como el vigilante); repite R veces añadiendo N líneas; borra el archivo al terminar (también si falla, vía guard `Drop`).
  - **queries**: las 5 consultas del bench y la "llamada completa del Resumen" (la lista de `useDashboardData`, secuencial, como la ejecutan los comandos con el `Mutex` único).
- Memoria pico: `VmHWM` de `/proc/self/status` (std, sin dependencias); fuera de Linux imprime `n/d`.
- Salida: tabla de texto estable (para copiar a `BASELINE.md`).

### D7. Perfiles de compilación (en `src-tauri/Cargo.toml`, raíz del workspace)
```toml
[profile.release]
lto = "fat"
codegen-units = 1
opt-level = 3
panic = "abort"
strip = true

[profile.profiling]
inherits = "release"
debug = true
strip = false
```
- Test de configuración `src-tauri/tests/build_profiles.rs`: lee `Cargo.toml` como texto y comprueba las claves de ambas secciones (parser mínimo por secciones; sin añadir `toml` como dependencia).
- `cargo bench` usa el perfil `bench` (hereda de `release`); cargo ignora `panic` en `bench`/`test`, así que criterion funciona.

### D8. Flamegraphs
- `cargo flamegraph --profile profiling --example perf_scan -o docs/perf/cold-scan.svg -- --fixtures <1g> --phase cold`.
- `live-refresh.svg`: `--phase live --repeat 200`, para que el refresco domine las muestras frente al escaneo frío previo (se documenta la contaminación residual). Alternativa considerada: `perf record --delay`/marcadores; más frágil.
- Si `perf` no funciona en distrobox (p. ej. `perf_event_paranoid` del host), se documenta en `BASELINE.md` el motivo, el valor observado y el comando exacto para reproducirlo en el host; el ranking se apoya entonces en spans y benches.

### D9. `docs/perf/BASELINE.md`
- Secciones: Entorno (CPU, cores, RAM, SO, versión de rustc, commit), Reproducción (comandos exactos), Escaneo frío 50 MB/200 MB/1 GB, Refresco en vivo, Consultas del Resumen (5 + llamada completa), Memoria pico, Criterion, Flamegraphs, Top 5 cuellos de botella (posición, componente, evidencia: % del flamegraph, span o bench, hipótesis de fase que lo ataca).

## Risks / Trade-offs

- [`panic = "abort"` cambia el modo de fallo: hoy un pánico en el hilo de escaneo o del vigilante solo mata ese hilo; con abort cierra la app entera] → Lo exige el perfil pedido; se mitiga con la regla de no añadir `unwrap()` en ingesta y revisando en la tarea del perfil los `unwrap`/`expect` existentes en hilos de fondo (se listan en `BASELINE.md` como riesgo; corregirlos queda fuera de esta fase si implica cambio de comportamiento).
- [LTO fat + 1 codegen unit alarga mucho la compilación release] → Solo afecta a `release`/`profiling`/`bench`; `dev` sin cambios.
- [Los fixtures sintéticos pueden no reflejar la distribución real] → Plantillas derivadas de los fixtures reales del repo; se documenta el reparto en `BASELINE.md`; un test de ingesta sobre un fixture pequeño exige cero errores.
- [Spans en consultas añaden overhead medible con trazas activas] → Las mediciones de la línea base se toman sin `AGENTBOARD_LOG`; los spans se usan solo para atribuir.
- [`perf` no disponible en contenedor] → Fallback D8.
- [Generar 1 GB tarda y ocupa disco] → Solo bajo demanda, en `target/` (ignorado).
- [Inicializar trazas en el MCP podría escribir en stdout] → El writer es siempre stderr; test que lo fija.

## Migration Plan

- Sin migración de datos. Despliegue normal con la siguiente versión.
- Rollback: revertir el commit; los perfiles y las trazas no dejan estado.

## Open Questions

- Ninguna que cambie specs, enfoque o tareas. Decisiones tomadas de forma autónoma: nivel por defecto `warn` (D2), salida de fixtures bajo `src-tauri/target/` (D4), las 5 consultas elegidas (D5), y la estrategia de `--repeat` para el flamegraph en vivo (D8).

## Dependencias añadidas (justificación)

- `tracing` 0.1 (crate `agentboard` y `agentboard-mcp`): spans con duración y macros de nivel; estándar de facto del ecosistema (tokio-rs), ligero (sin dependencias pesadas) y mantenido activamente. En el MCP solo se usa para `error!`.
- `tracing-subscriber` 0.3 con `default-features = false` y features `env-filter`, `fmt`, `std`: filtro por directivas (`AGENTBOARD_LOG`) y salida de texto a stderr; sin `ansi`, `tracing-log` ni `smallvec`, para no arrastrar más de lo necesario (añade `regex`/`matchers` por `env-filter`); mismo mantenedor que `tracing`.
- `criterion` 0.8 (solo dev-dependency) con `default-features = false` + `cargo_bench_support`: benchmarks estadísticos estables sin `plotters` ni `rayon`; mantenido por la organización bheisler/criterion-rs. No afecta al binario distribuido.

## Desviaciones durante la implementación

- **Sin colores ANSI nunca** (D1 decía "sin ANSI cuando no es TTY"): se desactiva la feature `ansi` de `tracing-subscriber` para aligerar la dependencia; la salida es la misma en terminal y fuera de ella.
- **"El escritor por defecto es stderr"** (tarea 2.2) se prueba de forma más fuerte en `src-tauri/mcp/tests/salida_stdio.rs`: se lanza el binario MCP real con un `HOME` vacío y `AGENTBOARD_LOG` inválido o `debug`, y se comprueba que stdout solo tiene JSON-RPC y que avisos y spans salen por stderr. Sustituye también la prueba manual de la tarea 2.4.
- **Helper de test `several_dispatchers`** (`telemetry::tests`): `tracing-core` calcula el interés de un callsite nuevo con el dispatcher del hilo que lo registra cuando solo hay un dispatcher vivo (`has_just_one`); con tests en paralelo, otro hilo sin subscriber lo marcaba como «nunca» y el subscriber del test perdía spans (fallo intermitente ~50 %). Mantener un segundo dispatcher permanente en los tests obliga a consultar a todos los vivos. Solo afecta a tests.
- **Test del vigilante** `relee_al_cambiar_un_log_sin_duplicar`: tenía una carrera previa (las filas se ven al hacer commit, el aviso a la UI llega justo después de soltar el `Mutex`). Se corrige solo el test (espera ambas condiciones); sin cambio de producto.
- **Nombres de campos** de los spans: `ruta` y `raiz` (en español, como el resto de mensajes), `files`, `lines`, `records`, `bytes`, `errors`, `by`, `bucket`, `format`, `origen`. `sessions::list_projects` se instrumenta como `list_project_summaries` para no confundirse con `queries::list_projects`. Además de las consultas de D3 se instrumentan `pricing::apply_overrides` y `pricing::list_prices` (los invocan comandos). `findings::compute` no se instrumenta: ningún comando de la app lo invoca (solo el MCP).
- **Generador**: tamaños en múltiplos de 1024 (`50m` = 50 MiB); escribe un archivo vacío `.gen-fixtures` como marca y solo borra una carpeta de salida existente si tiene esa marca. Codex sin subagentes; Cursor con marcas `<timestamp>` en UTC-7 como el fixture versionado. Las piezas puras se prueban importando el script con `importlib` (sin generar 50 MiB en el test).
- **Módulo común**: `benches/common/mod.rs` se comparte con `examples/perf_scan.rs` vía `#[path]` y sus tests viven en `src-tauri/tests/perf_common.rs` (los benches con `harness = false` no ejecutan `#[test]`).
- **Bench `summary_queries`** mide además la llamada completa del Resumen (18 consultas, como `useDashboardData` sin periodo anterior). Zona horaria fija `Europe/Madrid` y «mes en curso» = agosto de 2026 para no depender de la máquina ni de la fecha.
- **`perf_scan`** imprime para cada consulta la primera ejecución ("fría", tras el escaneo) y la mediana de 5 ejecuciones.
- **`panic = "abort"` y el guard de `perf_scan`**: con abort un pánico no ejecuta `Drop`; `perf_scan` propaga los errores con `Result` para que el guard borre el archivo de refresco en los fallos esperables.
- **Aviso de cargo `output filename collision`** al compilar benches: viene de `crate-type = ["staticlib", "cdylib", "rlib"]` (el perfil `bench` compila la lib para el harness y como dependencia). Es solo un aviso; no se toca el `crate-type` en esta fase (cambiarlo afecta al empaquetado de Tauri).
- **Flamegraphs con punteros de marco** (D8 proponía `cargo flamegraph`): `perf` funciona en distrobox (`perf_event_paranoid = 2`), pero el desenrollado DWARF de `cargo flamegraph` dejó el 70 % de las muestras en `malloc` sin llamadores y perdió datos. Se compila el perfil `profiling` con `-C force-frame-pointers=yes` en `target/fp` y se graba con `perf record --call-graph fp`; el SVG se genera con `flamegraph --perfdata`. Comandos exactos en `docs/perf/BASELINE.md`.
- **`live-refresh.svg` sobre 200 MB** (no 1 GB): con `--repeat 200` el refresco ocupa ~86 % de las muestras frente al escaneo frío previo; con 1 GB el escaneo frío (~90–110 s) habría dominado.
- **Tabla de consultas con `--phase queries`**: en `--phase all` las consultas se ejecutan tras el refresco en vivo, que mete 20 000 llamadas dentro del filtro; las cifras de referencia se toman sin esa contaminación (las dos se documentan).
- **Pruebas de mutación** (`cargo mutants --in-diff`, alcance del diff): 18 mutantes, casi todos de función entera por los atributos `#[instrument]`. Cada mutante recompila y prueba el crate de Tauri (~55 s), así que el límite de 3 minutos solo dio para 1 (`lib.rs::run → ()`: sobrevive; es el punto de entrada gráfico, no ejecutable sin pantalla). Los demás quedan cubiertos por tests que los detectarían (fixtures, spans, telemetría y el binario MCP), salvo `tray::refresh` y `watcher::spawn` (envoltorios de GUI/hilo cuyo único cambio es el `eprintln!` → `tracing`).
