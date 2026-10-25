# Design

## Context

- Motivación y alcance: proposal.md. Requisitos: `specs/rendimiento/spec.md` de este cambio.
- CI actual (`.github/workflows/ci.yml`): job `frontend` y matriz `rust` (ubuntu-22.04, windows, macos). En Linux instala WebKitGTK y ejecuta `npm run build` antes de cargo porque `generate_context!` (en `lib.rs`) necesita `dist/`. Los benches enlazan `agentboard_lib`, así que el job nuevo necesita lo mismo.
- Benches criterion 0.8 (`default-features = false`, sin gráficos): 17 benchmarks en 4 grupos — `parse_line/*` (5), `parse_file/*` (5), `ingest_file/claude_code_5mib`, `summary_queries/*` (6, con nombres como `breakdown(project)`). `benches/common/mod.rs` genera el fixture `50m` con `scripts/gen-fixtures.py` si falta.
- Salida de criterion comprobada en local: `src-tauri/target/criterion/<grupo>/<función>/new/benchmark.json` (`full_id`, p. ej. `parse_file/claude_code`) y `new/estimates.json` (`median.point_estimate` en ns). Junto a `new/` pueden existir `base/` y `change/` de ejecuciones anteriores.
- Perfiles en `Cargo.toml`: `release` (LTO fat, 1 codegen unit), `profiling`, `bench-fast` (release sin LTO, 16 codegen units). No hay `[profile.dev]`.
- Las cifras de las Fases 0–4 están en `docs/perf/BASELINE.md` (portátil Ryzen 5 Pro 7535U, con carga variable). No sirven como línea base de CI: otro hardware.
- Herramientas disponibles sin dependencias nuevas: Python 3 (ya lo usa el generador y su test `scripts/test_gen_fixtures.py` con `unittest`), `actionlint` en la máquina de desarrollo.

## Goals / Non-Goals

**Goals:**
- Puerta de regresión determinista en su lógica (umbral, nombres, ausencias) y probada con tests unitarios.
- Línea base medida en el mismo tipo de runner que la compara, actualizable sin herramientas locales.
- Informe final que se pueda revisar sin leer las cinco secciones de `BASELINE.md`.

**Non-Goals:**
- Optimizaciones nuevas o volver a medir las Fases 0–4 (el informe usa las cifras ya documentadas).
- Ejecutar `scripts/perf-ui.mjs` o `perf_scan` en CI.
- Benchmarks en Windows/macOS, comparación estadística (tests de hipótesis) o histórico de resultados.
- Comentarios automáticos en el PR.

## Decisions

### D1. Calibración de la línea base: medida en el runner de CI y versionada

Elegido: `docs/perf/baseline.json` lo produce el propio job `bench` en `ubuntu-22.04`. Cada ejecución del job (PR, push a `main` o `workflow_dispatch`) escribe una línea base candidata con los resultados de esa ejecución y la sube como artefacto `perf-baseline` (con `if: always()`, también cuando la comparación falla). Actualizar la línea base = descargar el artefacto de una ejecución en el estado deseado (`gh run download <id> -n perf-baseline`) y versionarlo en un commit propio (`perf(ci): actualiza la línea base de benchmarks`), con el motivo en el mensaje.

Alternativas:
- **Comparar contra la merge-base en el mismo job** (compilar y medir dos commits): inmune a la diferencia de hardware entre runners, pero duplica compilación y medición, y no hay línea base versionada (el requisito la exige). Descartada.
- **Línea base de la máquina de desarrollo**: otro hardware; el umbral del 20 % no tendría sentido. Descartada.
- **Job separado solo con `workflow_dispatch` para capturar**: añade un segundo camino que puede divergir del que compara (perfil, flags). Con el artefacto en cada ejecución, captura y comparación salen del mismo paso. `workflow_dispatch` se añade solo como disparador para poder regenerarla en `main` a demanda.

Ruido entre runners: los runners de GitHub no tienen siempre la misma CPU. La línea base guarda `cpu` (modelo de `/proc/cpuinfo`), `runner`, `commit`, `fecha` y `perfil`; la comparación avisa (sin fallar) si la CPU actual es distinta y lo incluye en el mensaje de fallo, para que quien lo vea distinga una regresión de un cambio de máquina. Ver Risks.

Arranque: mientras `docs/perf/baseline.json` no exista, la comparación falla con «no hay línea base» y las instrucciones; la primera ejecución en CI de esta rama publica la candidata, que se versiona (tarea 4.2).

### D2. Script de comparación en Python estándar

`scripts/bench-compare.py`, sin dependencias, con dos subcomandos:
- `capture --criterion-dir <dir> --out <json> [--commit <sha>] [--runner <nombre>] [--profile <perfil>]`: escribe la línea base.
- `compare --criterion-dir <dir> --baseline <json> [--threshold 0.20]`: imprime un informe Markdown (benchmark, línea base, actual, cambio, estado) y sale con 0 (sin regresiones), 1 (regresión o benchmark de la línea base ausente) o 2 (entrada no válida: sin línea base, sin resultados, JSON mal formado). El job añade el informe a `$GITHUB_STEP_SUMMARY`.

Lectura de criterion: recorre `<dir>/**/new/benchmark.json`, toma el nombre de `full_id` (nunca de la ruta, que criterion puede sanear) y la mediana de `new/estimates.json` → `median.point_estimate` (ns). Ignora `base/` y `change/`. Mediana y no media: menos sensible a valores atípicos de un runner compartido.

Regla: regresión si `actual > línea_base × (1 + umbral)`; exactamente +20 % no falla. Mejoras no fallan. Benchmark en la línea base y no en los resultados → fallo (si no, renombrar un bench desactivaría la protección en silencio). Benchmark nuevo → «sin línea base», no falla. Mensaje de fallo, una línea por bench: `REGRESIÓN summary_queries/activity: 5,41 ms → 6,89 ms (+27,4 %, umbral +20 %)` y al final cómo actualizar la línea base si el cambio es intencionado.

Formato de `baseline.json` (versionado, legible):

```json
{
  "version": 1,
  "statistic": "median",
  "unit": "ns",
  "captured": {"commit": "…", "date": "…", "runner": "ubuntu-22.04", "cpu": "…", "profile": "bench-fast"},
  "benchmarks": {"parse_line/claude_code": 2410.5, "summary_queries/breakdown(project)": 158000.0}
}
```

El umbral vive en el script (`--threshold`, por defecto 0,20), no en el JSON: un solo sitio.

Alternativas: `critcmp` o `cargo-criterion` (dependencias nuevas que instalar en CI); `--save-baseline`/`--baseline` de criterion (compara contra `target/`, no contra un archivo versionado). Descartadas.

### D3. Job `bench` en `ci.yml`

- `runs-on: ubuntu-22.04`, en `pull_request`, `push` a `main` y `workflow_dispatch` (se añade al `on:` del workflow).
- Pasos: checkout → WebKitGTK → Node 22 + `npm ci` + `npm run build` → toolchain estable → `Swatinem/rust-cache@v2` con `key: bench` → `python3 -m unittest scripts/test_bench_compare.py` → `python3 scripts/gen-fixtures.py 50m` → `cargo bench --manifest-path src-tauri/Cargo.toml --profile bench-fast --bench parse_line --bench ingest_file --bench summary_queries` → `capture` a `src-tauri/target/baseline-candidate.json` (`if: always()`) → `upload-artifact` `perf-baseline` (`if: always()`) → `compare` contra `docs/perf/baseline.json`.
- Captura antes que comparación para que la candidata exista aunque la comparación falle.
- Perfil `bench-fast` (release sin LTO): compila en una fracción del tiempo de `release` con LTO fat en un runner de 4 núcleos. La línea base se mide con el mismo perfil (queda en `captured.profile`), así que la comparación es coherente; las regresiones que solo aparecen con LTO quedan fuera (ver Risks). Las cifras de `BASELINE.md` (perfil `bench`) no se comparan con las de CI.
- `timeout-minutes: 45` para que un runner atascado no bloquee.

### D4. Perfil de desarrollo

- `[profile.dev.package."*"] opt-level = 3`: siempre (las dependencias se compilan una vez y se cachean; `tauri dev` y `cargo test` van más rápidos con SQLite y serde optimizados). No afecta a scripts de build ni macros (`build-override`).
- `[profile.dev] opt-level = 1` para el código propio: solo si la recompilación incremental no empeora más de un 20 % (mismo umbral que la puerta de CI). Medición: `touch src-tauri/src/lib.rs && time cargo build --workspace` tres veces con y sin la línea, mediana; anotar en «Desviaciones» y, si no cumple, no añadirla.

### D5. Documentación

- Diagrama de hilos y canales en Mermaid (GitHub lo pinta) en `docs/wiki/Arquitectura.md`, sección nueva «Hilos y canales de la ingesta»: `notify` → vigilante (filtro de eventos, debounce por archivo 150 ms / 1 s) → `Job::Rescan` por canal → hilo de ingesta (escritor único, lotes) → `derived::refresh` → generación + cambios → `notifier` (throttle 1/s) → evento `data:changed` → caché de consultas de la interfaz (generación) → `get_dashboard_snapshot` por el `ReadPool`. Cada flecha con el nombre del canal o del evento.
- Guía «Cómo perfilar» en `CONTRIBUTING.md` (no se duplica el diagrama: se enlaza): `AGENTBOARD_LOG`, `gen-fixtures.py`, `perf_scan` y sus fases, `cargo bench`, flamegraph con punteros de marco (perfil `profiling`), `perf-ui.mjs`, y «Benchmarks en CI y línea base» (cómo leer el fallo y cómo actualizar `baseline.json`, D1). Los comandos se toman de «Reproducción» de `BASELINE.md`, que sigue siendo la referencia completa. La sección «Integración continua» de `CONTRIBUTING.md` y la de `Arquitectura.md` mencionan el job `bench`.
- `README.md`: una línea en «Desarrollo» que enlaza `docs/perf/RESULTS.md`.

### D6. `docs/perf/RESULTS.md` con cifras ya documentadas

Sin mediciones nuevas: cada cifra cita su sección de `BASELINE.md` (fase y commit). «Antes» = primera medición de la métrica (Fase 0 o la partida de la fase que la introdujo); «Después» = última medición. Mapa de partida (la implementación lo verifica contra `BASELINE.md` y corrige si difiere):

| Presupuesto | Antes | Después | Estado |
|---|---|---|---|
| Arranque con 200 MB indexados < 1,5 s | 1,8 s, releyendo todo (partida Fase 2) | 129–134 ms (Fase 3) | Cumple |
| Escaneo frío 200 MB < 10 s | 15,5 s (Fase 0) | 2,14–2,18 s en memoria (Fase 3); 2,78–2,85 s a disco (Fase 2) | Cumple |
| Escritura → Resumen < 500 ms (mediana) | 787–789 ms (partida Fase 3) | 459–462 ms (Fase 3) | Cumple |
| ≤ 1 aviso/s, sin escaneos completos | 1 aviso al final, 1 completo (partida Fase 3) | 4 avisos, separación ≥ 1 000,1 ms, 0 completos | Cumple |
| Cambio de filtro/apartado cacheado < 100 ms | 66,5–166,7 ms (partida Fase 4) | 33,3 ms (Fase 4) | Cumple |
| Cambio de filtro/apartado en frío < 300 ms | 166,8–216,7 ms (partida Fase 4) | 133,3–133,4 ms con latencia de 200 MB; ~650 ms con la de 1 GB | Cumple (200 MB) |
| Resumen 200 MB < 300 ms frío / < 100 ms caché | 1 012 ms (Fase 0) | 102–104 ms / 0,0 ms (Fase 3) | Cumple |
| Cada consulta 1 GB < 300 ms | 274–1 001 ms (Fase 0) | `activity_daily` 238–243 ms, la mayor (Fase 3) | Cumple |
| CPU en reposo ~0 % | 0 ms en 60 s (partida Fase 3) | 0–10 ms en 60 s, 0 relecturas | Cumple |
| Memoria acotada | RSS 72,6 MiB vivo 200 MB (Fase 0); WAL 1 255 MiB | RSS 88/323 MiB (Fase 2); respuestas ≤ 246 KiB; WAL 1 255 MiB con lector continuo | Parcial (WAL) |
| Refresco con lecturas ≤ ×1,1 | ×1,29 (partida Fase 3) | ×1,21–1,23 | No cumple |
| Perfil de distribución | sin perfil propio | LTO fat, 1 codegen unit, `panic = "abort"` (Fase 0) + dev con dependencias optimizadas (esta fase) | Cumple |

«Lo que no se hizo o no se cumplió» enumera, con cifra y motivo: contención/WAL con lecturas largas en la prueba sintética extrema (×1,22, WAL 1,2 GiB); frío +22–29 % por la caché en disco; D10 (2 de 24 secuencias de reescaneo incremental difieren del escaneo desde cero); mutation testing parcial (Fase 0 sin puntuación, Fases 1–3 sobre subconjuntos, Fase 4 92,6 % con mutador propio porque Stryker no activó mutantes con vitest 5); `DailyPanel`/`ActivityPanel` ampliado sin uso; agrupación mensual en Rust descartada; coste IPC de series largas sin medir con backend real. «Próximos pasos»: los seguimientos abiertos de las Fases 3–4 en orden de impacto. «Reproducción»: enlace a `BASELINE.md` («Reproducción» y cada «Reproducir»), `perf-ui.mjs` y el job `bench`.

### D7. Mutación del script

Rama propia del script de comparación (`scripts/bench-compare.py`): mutador de operadores propio como en la Fase 4 (comparaciones, constantes, booleanos, `return`), corte a 3 min. Objetivo > 95 %; supervivientes equivalentes justificados en «Desviaciones». Sin mutación en YAML ni Markdown.

## Risks / Trade-offs

- [CPU distinta entre runners de GitHub → falsos positivos con +20 %] → Mediana en vez de media; aviso con la CPU de la línea base y la actual en el mensaje; si un fallo coincide con CPU distinta, volver a ejecutar el job; si se repite con otra CPU habitual, regenerar la línea base (D1). Si los falsos positivos son frecuentes, el siguiente paso es comparar contra la merge-base en el mismo job (alternativa descartada en D1), sin cambiar el formato del informe.
- [Microbenchmarks de µs (`parse_line/*`) más ruidosos que los de ms] → Se mantiene el umbral único del 20 % del requisito; si alguno da falsos positivos repetidos, anotarlo y valorar un umbral por grupo (cambio de spec).
- [`bench-fast` no detecta regresiones que solo aparecen con LTO] → Aceptado: la comparación de distribución sigue siendo manual con `perf_scan` y `BASELINE.md`.
- [Tiempo de CI: compilar benches y ~3–5 min de medición por PR] → Solo Linux, `rust-cache` con clave propia, `bench-fast`, `timeout-minutes`.
- [Arranque: el primer PR queda en rojo hasta versionar la línea base] → Esperado y documentado; la candidata se publica aunque falle.
- [`opt-level = 3` en dependencias alarga la compilación limpia de dev] → Una vez por caché; CI usa `rust-cache`.
- [El informe cita cifras de máquinas con carga variable] → Cada cifra lleva su fase, commit y rango; no se presentan como medidas en una sola tanda.

## Migration Plan

1. Versionar perfil, script, tests, job y documentación.
2. Primera ejecución del job en la rama: falla con «no hay línea base» y publica `perf-baseline`.
3. Versionar el artefacto como `docs/perf/baseline.json`; la siguiente ejecución queda en verde.
4. Rollback: quitar el job `bench` de `ci.yml` (el resto es documentación y un perfil de dev).

## Desviaciones durante la implementación

### D4: medición del perfil de desarrollo (tareas 1.1–1.2)

Distrobox `agentburn-dev`, Ryzen 5 Pro 7535U con carga variable, compilación ya caliente; `touch src-tauri/src/lib.rs && cargo build --workspace` tres veces.

| Perfil dev | Recompilación incremental (3 tandas) | Mediana | `touch` + `cargo test --workspace` |
|---|---|---|---|
| Partida (sin `[profile.dev]`) | 7,12 / 5,44 / 5,27 s | 5,44 s | 25,8 s |
| `opt-level = 1` + dependencias `opt-level = 3` | 5,70 / 6,34 / 5,07 s | 5,70 s | 20,3 s |

+4,8 % en la mediana, por debajo del 20 %: se conserva `opt-level = 1`. `cargo test --workspace` en verde (265 + 62 tests); `[profile.release]` sin cambios (`git diff src-tauri/Cargo.toml` solo añade las dos secciones dev).

### D7: mutación de `scripts/bench-compare.py` (tarea 2.3)

`mutmut` y `cosmic-ray` no están instalados (sin red para instalarlos sin dependencias nuevas), así que se usó un mutador de operadores propio sobre el AST (fuera del repositorio): comparaciones (`>`↔`>=`, `<`↔`<=`, `==`↔`!=`, `in`↔`not in`, `is`↔`is not`), constantes numéricas (`0`→`1`, resto→`0`), booleanos, `and`↔`or`, quitar `not`, aritmética (`+`↔`-`, `*`↔`/`), `return x`→`return None` y negar la condición de las expresiones condicionales; un mutante por sitio, la suite completa por mutante, corte a 3 min.

- Primera pasada: 62 mutantes, 49 muertos (79,0 %, 8,8 s). Huecos reales: `cpu_model` sin test (se le inyecta la ruta de `cpuinfo` con `/proc/cpuinfo` por defecto), el formato legible del JSON capturado (`indent`, `ensure_ascii`), la creación de carpetas anidadas y la sobrescritura de la candidata, y los argumentos obligatorios de la CLI. Se añadieron 5 tests (22 en total).
- Segunda pasada: 62 mutantes, 61 muertos (**98,4 %**, 9,7 s). Superviviente equivalente: `__doc__.splitlines()[0]` → `[1]` en la descripción de `argparse`; solo cambia el texto de `--help`.

### D3: dos pasos añadidos al job `bench` (tarea 3.1)

- `rm -rf src-tauri/target/criterion` antes de `cargo bench`: `rust-cache` restaura `target/` y unos `new/` de una ejecución anterior ocultarían un benchmark eliminado o renombrado (no saldría «AUSENTE»).
- `shell: bash` en el paso de comparación: el shell por defecto de `run` es `bash -e` sin `pipefail`, y el `| tee -a "$GITHUB_STEP_SUMMARY"` se tragaría el código 1/2 del comparador.

`actionlint .github/workflows/ci.yml` y `yaml.safe_load` sin errores; los jobs `frontend` y `rust` no cambian.

### Ensayo local del job (tarea 3.2)

En distrobox, mismos comandos que el job: `gen-fixtures.py 50m` (1,4 s, 397 archivos), `rm -rf src-tauri/target/criterion`, `cargo bench --profile bench-fast --bench parse_line --bench ingest_file --bench summary_queries` (compilación 2 min 31 s con dependencias ya compiladas en otros perfiles; total 322 s ≈ 5,4 min). Salen **17 benchmarks**: el target `parse_line` incluye también el grupo `parse_file/*`. `capture` → código 0; `compare` contra su propia captura → 0; contra una copia con `summary_queries/activity` dividida por 1,25 → código 1 con `REGRESIÓN summary_queries/activity: 4,07 ms → 5,09 ms (+25 %, umbral +20 %)` y las instrucciones de actualización. Reaparece el aviso de cargo `output filename collision` ya documentado en la Fase 0 (`crate-type`); no afecta al job.
