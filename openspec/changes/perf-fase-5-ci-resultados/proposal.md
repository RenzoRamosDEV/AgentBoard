# Proposal

## Why

Las Fases 0–4 dejaron el backend y la interfaz dentro de casi todos los presupuestos, pero nada impide que un cambio futuro los pierda sin que nadie lo note: los benchmarks criterion solo se ejecutan a mano, las cifras están repartidas por las secciones de `docs/perf/BASELINE.md` y la arquitectura de hilos y canales y la forma de perfilar no están escritas en un solo sitio. Esta fase de cierre protege lo conseguido y deja los resultados y la guía documentados, sin optimizaciones nuevas.

## What Changes

- Perfil de desarrollo: dependencias compiladas con `opt-level = 3` (`[profile.dev.package."*"]`) y, si la medición lo permite, el código propio con `opt-level = 1` en dev.
- Job de CI `bench` (solo Linux, `ubuntu-22.04`): genera el fixture de 50 MB con `scripts/gen-fixtures.py`, ejecuta los benchmarks criterion existentes (`parse_line`, `ingest_file`, `summary_queries`) y los compara con una línea base versionada `docs/perf/baseline.json`; falla con un mensaje claro si algún benchmark empeora más de un 20 %.
- Script sin dependencias nuevas (`scripts/bench-compare.py`, Python estándar, con tests unitarios) que lee la salida de criterion, compara y también genera la línea base candidata; el job la publica como artefacto en cada ejecución para que actualizarla sea copiar un archivo medido en el mismo tipo de runner.
- Documentación: diagrama de hilos y canales de la ingesta en `docs/wiki/Arquitectura.md`, guía «Cómo perfilar» y «Cómo actualizar la línea base» en `CONTRIBUTING.md`.
- `docs/perf/RESULTS.md`: antes/después y estado de cada presupuesto del objetivo global, comandos de reproducción, lo que no se hizo o no se cumplió y por qué, y próximos pasos.

## Capabilities

### New Capabilities
<!-- ninguna -->

### Modified Capabilities
- `rendimiento`: añade la protección contra regresiones en CI con línea base versionada, el perfil de desarrollo con dependencias optimizadas, el informe final de resultados y la documentación de la arquitectura de ingesta y de cómo perfilar.

## Impact

- Build: `src-tauri/Cargo.toml` (perfil `dev`).
- CI: `.github/workflows/ci.yml` (job `bench`, disparador `workflow_dispatch`).
- Scripts: `scripts/bench-compare.py` y `scripts/test_bench_compare.py` (solo biblioteca estándar).
- Datos versionados: `docs/perf/baseline.json`.
- Documentación: `docs/perf/RESULTS.md`, `docs/wiki/Arquitectura.md`, `CONTRIBUTING.md`, `README.md` (enlace a resultados).
- Código de la app y del servidor MCP: sin cambios. Dependencias de Rust y `package.json`: sin cambios.
