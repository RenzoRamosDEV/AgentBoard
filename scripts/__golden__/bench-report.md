## Benchmarks frente a la línea base

Línea base: commit `abc1234`, runner `ubuntu-22.04`, CPU `AMD EPYC 7763 64-Core Processor`, perfil `bench-fast`, 2026-10-25T10:00:00Z. Estadístico: mediana. Umbral: +20 %.

| Benchmark | Línea base | Actual | Cambio | Estado |
|---|---|---|---|---|
| ingest_file/claude_code_5mib | 41,20 ms | 39,00 ms | -5,34 % | ok |
| parse_line/claude_code | 2,41 µs | 2,89 µs | +20 % | ok |
| parse_line/gone | 900,00 ns | — | — | AUSENTE |
| parse_line/nuevo | — | 850,00 ns | — | sin línea base |
| summary_queries/activity | 5,41 ms | 6,89 ms | +27,4 % | REGRESIÓN |
| summary_queries/breakdown(project) | 158,00 µs | 158,00 µs | 0 % | ok |

> Aviso: CPU distinta de la de la línea base (`AMD EPYC 7763 64-Core Processor` → `Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz`). Si el job falla solo por eso, vuelve a ejecutarlo; si se repite con esta CPU, regenera la línea base.

- REGRESIÓN summary_queries/activity: 5,41 ms → 6,89 ms (+27,4 %, umbral +20 %)
- AUSENTE parse_line/gone: está en la línea base y no en esta ejecución; actualiza la línea base.

Si el cambio es intencionado, actualiza la línea base: descarga el artefacto `perf-baseline` de esta ejecución (`gh run download <id> -n perf-baseline`) y versiónalo como `docs/perf/baseline.json` en un commit propio (ver «Benchmarks en CI y línea base» en `CONTRIBUTING.md`).
