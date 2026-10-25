# Tasks

Comandos de Rust dentro de distrobox: `distrobox enter agentburn-dev -- bash -lc 'export PATH=$HOME/.cargo/bin:$PATH; cd src-tauri && <cmd>'`.
**TDD**: en cada tarea que cambia comportamiento, primero el test en rojo (verificar que falla), luego el código mínimo en verde y después refactor.
Sin optimizaciones nuevas en la app ni en el servidor MCP; sin dependencias nuevas en `Cargo.toml` ni en `package.json`.

## 1. Perfil de desarrollo (D4)

- [x] 1.1 Medir la partida: `touch src-tauri/src/lib.rs && time cargo build --workspace` tres veces (mediana) y `time cargo test --workspace`; anotar en design.md («Desviaciones»)
- [x] 1.2 Añadir `[profile.dev.package."*"] opt-level = 3` y `[profile.dev] opt-level = 1`; repetir la medición de 1.1; conservar `opt-level = 1` solo si la recompilación incremental no crece > 20 %, si no quitarla; anotar cifras y decisión en «Desviaciones»; verificar `cargo test --workspace` en verde y que `[profile.release]` no cambia (`git diff src-tauri/Cargo.toml`)

## 2. Script de comparación (D2)

- [x] 2.1 Tests en rojo en `scripts/test_bench_compare.py` (`unittest`, árbol falso de criterion en un directorio temporal con `new/`, `base/` y `change/`): lectura por `full_id` (incluido `summary_queries/breakdown(project)`) e ignorando `base/`/`change/`; `compare` con mejora, igual, +19,99 %, exactamente +20 % (pasa), +20,01 % y +25 % (fallan, código 1, mensaje con nombre, ambos valores y porcentaje); bench de la línea base ausente (código 1, pide actualizar la línea base); bench nuevo («sin línea base», código 0); sin archivo de línea base, carpeta sin resultados y JSON mal formado (código 2); aviso de CPU distinta sin fallar; `capture` escribe `version`, `statistic`, `unit`, `captured` y `benchmarks`, y su salida usada como línea base da código 0; golden del informe Markdown en `scripts/__golden__/bench-report.md`; verificar con `python3 -m unittest scripts/test_bench_compare.py` que fallan
- [x] 2.2 Implementar `scripts/bench-compare.py` (solo biblioteca estándar, subcomandos `capture` y `compare`, `--threshold` 0,20 por defecto); verificar 2.1 en verde y, en local sobre `src-tauri/target/criterion` real, que `capture` + `compare` contra su propia salida da código 0
- [x] 2.3 Mutación dirigida (D7) del script con un mutador de operadores propio, corte a 3 min; verificar > 95 % o supervivientes equivalentes justificados en «Desviaciones»

## 3. Job de benchmarks en CI (D1, D3)

- [x] 3.1 Añadir a `.github/workflows/ci.yml` el disparador `workflow_dispatch` y el job `bench` de D3 (tests del script, fixture `50m`, `cargo bench --profile bench-fast` de los tres benches, `capture` + artefacto `perf-baseline` con `if: always()`, `compare` con informe en `$GITHUB_STEP_SUMMARY`, `timeout-minutes: 45`); verificar con `actionlint .github/workflows/ci.yml` y `python3 -c 'import yaml,sys; yaml.safe_load(open(".github/workflows/ci.yml"))'`
- [x] 3.2 Ensayo local del job: `python3 scripts/gen-fixtures.py 50m`, el mismo `cargo bench --profile bench-fast …`, `capture` y `compare` contra la captura (código 0) y contra una copia con una mediana dividida por 1,25 (código 1 y mensaje claro); anotar el tiempo total en «Desviaciones»

## 4. Línea base en el runner (D1)

- [x] 4.1 Documentar en `CONTRIBUTING.md` («Benchmarks en CI y línea base»: qué compara, cómo leer el fallo, cómo descargar `perf-baseline` con `gh run download <id> -n perf-baseline` y versionarlo como `docs/perf/baseline.json`) y mencionar el job en «Integración continua» de `CONTRIBUTING.md` y de `docs/wiki/Arquitectura.md`; verificar que los comandos documentados coinciden con los del job
- [ ] 4.2 Primera línea base: ejecutar el workflow en la rama (push o `gh workflow run ci.yml --ref perf/rendimiento-fluido`), comprobar que `bench` falla con «no hay línea base» y publica `perf-baseline`, versionar el artefacto como `docs/perf/baseline.json` (commit `perf(ci): …`) y verificar que la siguiente ejecución de `bench` queda en verde; anotar commit, runner y CPU en «Desviaciones». Si no hay acceso a CI desde el entorno, dejar la tarea pendiente y avisar al usuario de que este paso es manual

## 5. Documentación y resultados (D5, D6)

- [x] 5.1 Añadir a `docs/wiki/Arquitectura.md` la sección «Hilos y canales de la ingesta» con el diagrama Mermaid de D5; verificar que cada nodo existe en el código (`watcher.rs`, `Job::Rescan`, `ingest_thread.rs`, `derived::refresh`, `notifier.rs`, `data:changed`, `src/lib/query.ts`, `get_dashboard_snapshot`, `ReadPool`) y que el bloque Mermaid es sintácticamente válido (p. ej. `npx -y @mermaid-js/mermaid-cli` puntual o vista previa de GitHub)
- [x] 5.2 Añadir a `CONTRIBUTING.md` la guía «Cómo perfilar» de D5 enlazando el diagrama; verificar ejecutando en local `AGENTBOARD_LOG=debug cargo run --example scan` (unos segundos), `perf_scan --fixtures target/perf-fixtures/50m --phase queries` con `bench-fast` y `node scripts/perf-ui.mjs --runs=1`
- [x] 5.3 Escribir `docs/perf/RESULTS.md` (D6): tabla por presupuesto con antes, después, estado y sección de origen en `BASELINE.md`, reproducción, «Lo que no se hizo o no se cumplió» con cifra y motivo, y próximos pasos; enlazarlo desde `README.md` («Desarrollo») y desde el inicio de `BASELINE.md`; verificar cada cifra contra `BASELINE.md` y que el documento no presenta como cumplido nada que `BASELINE.md` da como no cumplido

## 6. Comprobaciones finales

- [ ] 6.1 `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm test`, `npm run build`, `python3 -m unittest scripts/test_bench_compare.py scripts/test_gen_fixtures.py` y `actionlint` en verde; `git diff --stat 5c5e139 -- src/ src-tauri/src src-tauri/mcp` (archivo de la Fase 4) sin cambios; `openspec validate perf-fase-5-ci-resultados --strict` válido

## Workflow follow-up

- Archivar el cambio (`/opsx:archive`) tras la revisión y con la tarea 4.2 completada.
