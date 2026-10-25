# Resultados de rendimiento (Fases 0–5)

Resumen del trabajo de rendimiento de la rama `perf/rendimiento-fluido`: estado de cada
presupuesto, de dónde sale cada cifra, lo que no se hizo o no se cumplió y los próximos pasos.
**No hay mediciones nuevas**: cada cifra es de [`BASELINE.md`](BASELINE.md), que sigue siendo la
referencia completa (entorno, comandos y tablas de cada fase).

Todas las cifras son del mismo portátil (AMD Ryzen 5 Pro 7535U, 14 GiB, distrobox
`agentburn-dev`) con datos **sintéticos** de `scripts/gen-fixtures.py` y carga variable de la
máquina: por eso se dan rangos y no una tanda única. Perfil `release` salvo donde se indica.
Las de la interfaz son del arnés `scripts/perf-ui.mjs` con el mock y latencia simulada.

«Antes» es la primera medición de la métrica: la Fase 0 (`cd5d4fa`) o la partida de la fase que
la introdujo. «Después» es la última medición. Abreviaturas de origen en `BASELINE.md`:
**F0** «Resultados de `perf_scan`» y «Frente a los presupuestos del proyecto»; **F1** «Después de
la Fase 1»; **F2** «Después de la Fase 2»; **F3** «Después de la Fase 3»; **F4** «Después de la
Fase 4»; todas en su tabla «Antes y después» salvo indicación.

## Presupuestos

| Presupuesto | Antes | Después | Estado | Origen |
|---|---|---|---|---|
| Arranque con 200 MB indexados (hasta datos del Resumen) < 1,5 s | 1,8 s: había que releer todo (partida F2, `d7fa167`) | **129–134 ms** (F3; 138–144 ms en F2) | Cumple | F2, F3 |
| Escaneo frío 200 MB < 10 s | 15,5 s (15,5–16,9) | **2,14–2,18 s** en memoria (F3); **2,78–2,85 s** a la caché en disco (F2) | Cumple | F0, F2, F3 |
| Vigilante: escritura aislada → Resumen que la incluye < 500 ms (mediana) | 787–789 ms, p95 794–802 ms (partida F3, `97088e3`) | **459–462 ms**, p95 464–477 ms | Cumple | F3 |
| Avisos a la interfaz ≤ 1/s y sin escaneos completos (ráfaga de 500 líneas en 50 logs) | 1 aviso al final, 1 escaneo completo (partida F3) | **4 avisos**, separación mínima **1 000,1 ms**, 500 relecturas dirigidas, **0** completos | Cumple | F3 |
| Refresco en vivo: mediana de 200 refrescos de 1000 líneas < 200 ms; últimos 20 ≤ 1,5 × primeros 20 | 515 ms; 170 → 1 046 ms, crecía con la sesión (F0, `profiling` bajo `perf`) | **37,9 ms**; ×1,14 (F2; 46–48 ms y ×0,97–1,10 en F1) | Cumple | F0, F1, F2 |
| Cambio de filtro o apartado ya cargado < 100 ms | 66,5 ms (S1) y 166,7 ms (S2), latencia de 200 MB (partida F4, `c732391`) | **33,3 ms** (S1 y S2; máx. 49,7 ms) | Cumple | F4 |
| Cambio de filtro o apartado en frío < 300 ms | 33,2–216,7 ms (S1, S2, S3, S5; partida F4) | **33,1–133,4 ms** (máx. 166,9) con la latencia del Resumen de 200 MB; 35,6–666,6 ms con la de 1 GB | Cumple con 200 MB (con 1 GB, S2/S3/S5 en 650–667 ms) | F4 |
| Resumen 200 MB < 300 ms en frío / < 100 ms desde caché | 1 012 ms primera, 1 244 ms mediana (F0) | **102–104 ms** con comparación en una petición (F3) / **0,0 ms** (caché de resultados, F2–F3) | Cumple | F0, F2, F3 |
| Cada consulta del Resumen sobre 1 GB < 300 ms | 274–1 001 ms, 4 de 5 por encima (F0) | la mayor, `activity_daily`: **238–243 ms** (F3; en F2 eran 341 ms y no cumplía) | Cumple | F0 «Frente a los presupuestos», F2, F3 |
| CPU en reposo ~0 (< 60 ms en 60 s y 0 relecturas) | 0 ms, 0 relecturas (partida F3) | **0–10 ms**, 0 relecturas | Cumple | F3 |
| Memoria acotada: RSS | 200 MB: 60,7 MiB solo frío; 1 GB con vivo: 275,5 MiB (F0, «Escaneo frío, refresco en vivo y memoria») | 200 MB frío: 88 MiB; 1 GB vivo: 323 MiB (F2) | Acotada, pero +45 % / +17 % (pipeline paralelo y cachés) | F0, F2 |
| Memoria acotada: respuestas ≤ 1 MiB | 126,5 / 234,6 KiB, 200 MB / 1 GB (partida F3) | **246,1 / 327,0 KiB** (el snapshot sustituye a 20 respuestas) | Cumple | F3 |
| Memoria acotada: WAL ≤ 8 MiB tras la tanda de contención | 1 255 MiB con lector `ui` (partida F3) | 1 255 MiB con lector `ui`; 48–135 MiB con lector `loop` | **No cumple** | F3 |
| Relecturas con lecturas como la interfaz ≤ ×1,1 | ×1,36–1,38 en disco (F2, primera medición); ×1,29 (partida F3, `contention --reader ui`) | **×1,21–1,23** | **No cumple** | F2 «Frente a los presupuestos», F3 |
| Resumen en frío después / antes de la tanda de contención ≤ ×1,2 | ×2,05–2,06 (partida F3) | ×2,07–2,22 | **No cumple** (mide sobre todo el crecimiento de los datos: 200 000 llamadas más) | F3 |
| Perfil de distribución | sin perfil propio (antes de `167a65b`) | `release`: LTO fat, 1 codegen unit, `opt-level = 3`, `panic = "abort"`, `strip` (F0) · dev: dependencias con `opt-level = 3` y código propio con `opt-level = 1` (Fase 5) | Cumple | F0 «Entorno»; Fase 5 abajo |

Otras cifras de interés (sin presupuesto propio):

- Benchmarks criterion, fixture `50m` (F0 tanda A → F2): `summary_queries/resumen_completo`
  154,3 → **19,3 ms**; `ingest_file/claude_code_5mib` 232,9 → **41,9 ms**;
  `parse_line/claude_code` 4,62 → **2,41 µs** (F0 «Criterion», F2).
- Escaneo frío 1 GB: 86,9 s (F0) → 17,9 s (F1); en memoria 12,9–13,0 s (F3).
- Interfaz: arranque 450 102 → **399 658 B** (−11 %); en gzip, 117 786 B frente a los 115 448 B
  del `index.js` único de la partida (+2 %: varios chunks pequeños comprimen peor; F4 «Bundle»).

## Fase 5: protección y herramientas

- **Job `bench` en CI** (`.github/workflows/ci.yml`, `ubuntu-22.04`): genera el fixture de 50 MB,
  ejecuta los 17 benchmarks criterion con `bench-fast` y compara la mediana de cada uno con
  `docs/perf/baseline.json` mediante `scripts/bench-compare.py`; falla si alguno empeora más de
  un 20 % o falta, y publica cada vez la línea base candidata (`perf-baseline`). Cómo leer el
  fallo y actualizar la línea base: «Benchmarks en CI y línea base» en
  [`CONTRIBUTING.md`](../../CONTRIBUTING.md).
- **Perfil de desarrollo**: la recompilación incremental tras tocar `lib.rs` pasa de 5,44 s a
  5,70 s de mediana (+4,8 %, por debajo del 20 % permitido) y `touch` + `cargo test --workspace`
  de 25,8 s a 20,3 s (medición en `design.md` del cambio `perf-fase-5-ci-resultados`).
- **Documentación**: diagrama de hilos y canales en
  [`docs/wiki/Arquitectura.md`](../wiki/Arquitectura.md#hilos-y-canales-de-la-ingesta) y guía
  «Cómo perfilar» en `CONTRIBUTING.md`.

## Lo que no se hizo o no se cumplió

- **Contención y WAL con lecturas largas** (prueba sintética extrema: 200 000 llamadas nuevas en
  una sola sesión dentro del filtro): relecturas ×1,21–1,23 frente a ×1,1, WAL de 1 255 MiB
  (≈1,2 GiB) frente a 8 MiB con un lector siempre abierto y Resumen ×2,07–2,22 tras la tanda. Con
  lecturas de ~2 s y un aviso por segundo siempre hay un lector y el checkpoint `TRUNCATE` se
  aplaza; con huecos entre lecturas el WAL baja a 48–135 MiB (F3).
- **Escaneo frío +22–29 %** frente a la partida de la Fase 2 (en memoria, 50 MB / 200 MB / 1 GB:
  +28 / +29 / +22 %): es el coste de mantener coste, día, primera llamada, agregados y la caché
  fiel a los logs. Sigue muy por debajo del presupuesto (F2).
- **Memoria**: RSS pico +45 % (200 MB, solo frío) y +17 % (1 GB con vivo) frente a la Fase 0
  por los archivos en vuelo del pipeline paralelo y las cachés (F1, F2).
- **D10 de la Fase 2**: en 2 de 24 secuencias forzadas del test de propiedad (borrar el log
  principal de una sesión cuyos subagentes ya se truncaron o rotaron) el escaneo incremental
  difiere de leer desde cero en los turnos de otra sesión; la relectura dirigida coincide con el
  escaneo completo en las 24 (`design.md` de `perf-fase-3-vigilante-ui`, F3 «Seguimientos»).
- **Mutation testing parcial**: Fase 0, 1 de 18 mutantes examinados (cada uno recompila el crate
  de Tauri, ~55 s); Fases 1–3 sobre subconjuntos del diff por el límite de 3 min (F1: 97 de 545,
  96,5 %; F2: 305 de 404, 97,1 %; F3: 75 viables, 98,7 %); Fase 4 **92,6 %** (88 de 95) con un
  mutador propio porque Stryker no activaba los mutantes con vitest 5, con los 7 supervivientes
  equivalentes; Fase 5 98,4 % (61 de 62) sobre el comparador, con mutador propio.
- **`DailyPanel` y la vista ampliada de `ActivityPanel`** no se usan en ninguna vista; se dejaron
  (F4, `design.md` de `perf-fase-4-frontend`).
- **Agrupación mensual en Rust (D8 de la Fase 4) descartada**: con tres años en «Todo» ahorraba
  ~5,5 ms de render (4 %), por debajo de la regla del 10 % (F4).
- **Coste IPC de series largas sin medir con backend real**: el mock no refleja serializar años
  de series diarias (F4).
- **Cambio en frío con la latencia de 1 GB** (650–667 ms en S2/S3/S5): el presupuesto de 300 ms
  se cumple con la de 200 MB; con 1 GB lo marca el propio Resumen (621–633 ms, F3).
- **Primera línea base de CI** (tarea 4.2 de la Fase 5): pendiente de la primera ejecución del job
  en GitHub; hasta versionar `docs/perf/baseline.json`, el job `bench` falla con «no hay línea
  base» (pasos en «Benchmarks en CI y línea base» de `CONTRIBUTING.md`).

## Próximos pasos

En orden de impacto:

1. **Contención con una sesión enorme**: agregados por hora y por turno para `timeseries(hour)`,
   `activity`, `activity_daily` y `breakdown(command)`; lecturas más cortas dejan huecos al
   checkpoint y acotan el WAL (F3 «Seguimientos para la Fase 4»).
2. **Versionar la primera línea base de CI** y vigilar falsos positivos por CPU distinta entre
   runners; si son frecuentes, comparar contra la merge-base en el mismo job.
3. **IPC del Resumen con «Todo»**: medir con el backend real el coste de serializar años de series
   diarias; si importa, retomar el cubo mensual en Rust (F4 «Seguimientos»).
4. **Frío +22–29 %**: aplazar el refresco de derivados al final del escaneo inicial y memorizar el
   día local por rango en el escritor (F2 «Seguimientos para la Fase 3»).
5. **D10 de la Fase 2**: resolver las 2 secuencias forzadas en las que el escaneo incremental
   difiere de leer desde cero.
6. **Gzip del arranque +2 %**: agrupar en `manualChunks` los módulos comunes pequeños (F4).
7. **Mutación**: completar la cobertura del diff de Rust (más de 3 min por ejecución) y probar
   otra versión de Stryker con vitest.

## Reproducción

- Backend: «Reproducción» de [`BASELINE.md`](BASELINE.md) (fixtures, `perf_scan` por fase,
  criterion y flamegraphs) y el «Reproducir» de cada fase.
- Interfaz: `node scripts/perf-ui.mjs --latency=104,630 --runs=3` (F4 «Reproducir»).
- Regresiones: el job `bench` de CI; en local, los mismos comandos de «Benchmarks en CI y línea
  base» en [`CONTRIBUTING.md`](../../CONTRIBUTING.md). Guía general: «Cómo perfilar» en el mismo
  archivo.
