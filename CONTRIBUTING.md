# Guía de contribución

AgentBoard es un proyecto de código abierto. Las contribuciones —correcciones, mejoras,
soporte para nuevos agentes o traducciones— son bienvenidas. Este documento resume cómo
está montado el proyecto y qué se espera de una contribución.

## Filosofía del proyecto

- **Solo lectura de los logs; caché local reconstruible.** AgentBoard lee los logs que los
  agentes ya dejan en el ordenador (nunca los modifica) y guarda lo leído en una caché SQLite
  reconstruible en la carpeta de datos del usuario (`~/.local/share/agentboard/cache.sqlite` en
  Linux, permisos solo para el usuario), o solo en memoria con el ajuste «Solo memoria». La caché
  solo contiene lo que ya está en los logs —nunca el texto de prompts ni respuestas—, debe dar lo
  mismo que leer los logs desde cero y puede borrarse sin perder nada: si está dañada o es de
  otra versión de esquema, se borra y se reconstruye. Para borrarla a mano, con la app cerrada,
  basta con eliminar `cache.sqlite`, `-wal` y `-shm`. Cualquier cambio debe respetar esa premisa.
- **Local y privado.** Nada sale de la máquina: sin proxy, sin claves de API, sin telemetría.
- **Especificación primero.** Los cambios se describen con [OpenSpec](https://github.com/Fission-AI/OpenSpec)
  antes de implementarse (ver `openspec/`).

## Entorno de desarrollo

Requisitos: Node.js 20+, Rust estable (rustup) y las dependencias de sistema de Tauri
(en Linux, WebKitGTK 4.1). El desarrollo se hace cómodo en un contenedor
(por ejemplo `distrobox`/`toolbox`) con esas dependencias.

```bash
npm ci                      # dependencias del frontend
npm run tauri dev           # levanta la app (Vite + Tauri) con recarga en caliente
```

Comprobaciones antes de abrir un PR:

```bash
# Frontend
npx tsc -b                  # tipos
npm test                    # vitest

# Backend (Rust)
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --bins -- -D warnings
cargo test  --manifest-path src-tauri/Cargo.toml
```

El servidor MCP se compila y prueba aparte:

```bash
cargo build --manifest-path src-tauri/Cargo.toml -p agentboard-mcp
cargo test  --manifest-path src-tauri/Cargo.toml -p agentboard-mcp
```

### Trazas (`AGENTBOARD_LOG`)

Sin configurar nada el backend solo escribe avisos y errores (nivel `warn`) en stderr. Para
ver los spans con duración del escaneo, la ingesta, los comandos y las consultas:

```bash
AGENTBOARD_LOG=debug npm run tauri dev
AGENTBOARD_LOG=debug cargo run --manifest-path src-tauri/Cargo.toml --example scan
AGENTBOARD_LOG=agentboard_lib::ingest=debug cargo run --manifest-path src-tauri/Cargo.toml --example scan
```

El valor sigue la sintaxis de directivas de `tracing_subscriber::EnvFilter` (`info`, `debug`,
`modulo=nivel`, separadas por comas). Un valor no válido se ignora y se usa `warn`.

## Cómo perfilar

Antes de optimizar, medir. El recorrido de un cambio en un log hasta la interfaz (vigilante,
hilo de ingesta, notificador, caché de consultas, lectores) está en el diagrama
[«Hilos y canales de la ingesta»](docs/wiki/Arquitectura.md#hilos-y-canales-de-la-ingesta); la
referencia completa de comandos y cifras es [`docs/perf/BASELINE.md`](docs/perf/BASELINE.md)
(«Reproducción» y cada «Reproducir»), y el resumen de resultados,
[`docs/perf/RESULTS.md`](docs/perf/RESULTS.md). Herramientas de Rust desde la raíz del repo (o
dentro de `distrobox` si así trabajas); `dist/` debe existir (`npm run build`).

1. **Trazas** (duración de cada span): `AGENTBOARD_LOG=debug cargo run --manifest-path
   src-tauri/Cargo.toml --example scan` (ver «Trazas» arriba).
2. **Datos sintéticos** (nunca los logs reales): `python3 scripts/gen-fixtures.py 50m` (también
   `200m`, `1g`) → `src-tauri/target/perf-fixtures/<tamaño>/`.
3. **`perf_scan`** (ejemplo de medición; desde `src-tauri/`). Perfil `bench-fast` (release sin
   LTO) para iterar y `release` para la cifra final:

   ```bash
   cargo build --profile bench-fast --example perf_scan
   ./target/bench-fast/examples/perf_scan --fixtures target/perf-fixtures/50m --phase queries
   ```

   Fases: `cold` (escaneo frío), `live` (refresco de 1000 líneas, `--repeat N`), `queries`
   (Resumen y apartados, tamaño de cada respuesta), `warm` (arranque con `--cache`),
   `contention` (`--reader ui|loop`), `watch` (vigilante, `--events N` para la ráfaga) e `idle`
   (`--idle-secs 60`); `--fingerprint`/`--query-fingerprint` para comprobar equivalencia. Sin
   `--phase`, frío + vivo + consultas.
4. **Benchmarks criterion** (fixture `50m`; si falta lo generan o lo indican):
   `cargo bench --manifest-path src-tauri/Cargo.toml --bench parse_line --bench ingest_file
   --bench summary_queries` (añade `--profile bench-fast` para compilar mucho antes, como CI).
5. **Flamegraph con punteros de marco** (las pilas DWARF salen rotas; ver «Flamegraphs» de
   `BASELINE.md`), desde `src-tauri/`:

   ```bash
   export CARGO_TARGET_DIR=target/fp RUSTFLAGS="-C force-frame-pointers=yes" CFLAGS="-fno-omit-frame-pointer"
   cargo build --profile profiling --example perf_scan
   perf record -F 499 --call-graph fp -o target/perf-cold.data -- \
     target/fp/profiling/examples/perf_scan --fixtures target/perf-fixtures/1g --phase cold
   flamegraph --perfdata target/perf-cold.data -o ../docs/perf/cold-scan.svg
   ```

6. **Interfaz**: `node scripts/perf-ui.mjs --latency=104,630 --runs=3` (build de producción con
   el mock y latencia simulada en Chromium headless; `CHROME` para elegir el navegador,
   `--no-build` para reutilizar la build). Mide cambio de apartado, de periodo y de filtro en
   frío y en caché, y el aviso `data:changed`.

### Benchmarks en CI y línea base

El job `bench` de `.github/workflows/ci.yml` (solo `ubuntu-22.04`; en cada Pull Request, en
cada push a `main` y a mano con `workflow_dispatch`) hace lo mismo que esto:

```bash
python3 -m unittest scripts/test_bench_compare.py
python3 scripts/gen-fixtures.py 50m
rm -rf src-tauri/target/criterion
cargo bench --manifest-path src-tauri/Cargo.toml --profile bench-fast --bench parse_line --bench ingest_file --bench summary_queries
python3 scripts/bench-compare.py capture --criterion-dir src-tauri/target/criterion \
  --out src-tauri/target/baseline-candidate.json --commit "$(git rev-parse HEAD)" --runner ubuntu-22.04 --profile bench-fast
python3 scripts/bench-compare.py compare --criterion-dir src-tauri/target/criterion --baseline docs/perf/baseline.json
```

- **Qué compara**: la mediana de cada benchmark (17: `parse_line/*`, `parse_file/*`,
  `ingest_file/*`, `summary_queries/*`) con la de `docs/perf/baseline.json`. Falla (código 1) si
  alguno es más de un 20 % más lento (exactamente +20 % pasa) o si uno de la línea base no
  aparece; un benchmark nuevo sale como «sin línea base» y no falla. Sin línea base, sin
  resultados o con JSON mal formado, código 2.
- **Cómo leer el fallo**: el informe (también en el resumen de la ejecución en GitHub) tiene una
  fila por benchmark y una línea por problema, p. ej.
  `REGRESIÓN summary_queries/activity: 5,41 ms → 6,89 ms (+27,4 %, umbral +20 %)`. Si el runner tiene
  una **CPU distinta** a la de la línea base, los tiempos no son comparables: las regresiones se
  muestran como aviso y no bloquean (un benchmark ausente sí bloquea). Solo fallan por tiempo las
  ejecuciones en la misma CPU que la línea base.
- **Actualizar la línea base** (cambio intencionado, benchmark renombrado o nuevo, otra CPU
  habitual): cada ejecución sube la candidata medida en el runner como artefacto
  `perf-baseline`, también cuando la comparación falla. Desde una ejecución en el estado deseado:

  ```bash
  gh run list --workflow ci.yml --branch <rama>      # id de la ejecución
  gh run download <id> -n perf-baseline -D /tmp/perf-baseline
  cp /tmp/perf-baseline/baseline-candidate.json docs/perf/baseline.json
  git add docs/perf/baseline.json
  git commit -m "perf(ci): actualiza la línea base de benchmarks" -m "<motivo>"
  ```

  No la generes en local: otro hardware deja el umbral sin sentido. El commit, la fecha, el
  runner, la CPU y el perfil de la medición quedan en `captured`.
- **Primera vez**: mientras `docs/perf/baseline.json` no exista, el job falla con «no hay línea
  base»; es lo esperado. Se versiona la candidata de esa primera ejecución como arriba y la
  siguiente queda en verde.

## Estructura

```
src/                  UI React + TypeScript (views/, components/, lib/)
src/assets/           todas las imágenes (icono de la app y logos del panel)
src-tauri/src/
  providers/          un módulo por agente (trait Provider)
  ingest.rs           lectura incremental por offsets, sin duplicar
  queries.rs          consultas agregadas del dashboard
  pricing.rs          precios por modelo (USD / millón de tokens)
  watcher.rs          vigilante de archivos en vivo
src-tauri/mcp/        crate del servidor MCP (stdio, JSON-RPC)
openspec/             propuestas y especificaciones
```

## Cómo se añade soporte para un agente nuevo

1. Crear un módulo en `src-tauri/src/providers/` que implemente el trait `Provider`
   (detección, rutas de logs y parseo de sus registros a los tipos comunes).
2. Registrarlo en `providers::all()`.
3. Añadir un fixture anonimizado en `tests/fixtures/<agente>/` y un test de ingesta.
4. Normalizar sus nombres de herramienta/modelo a los de Claude Code cuando aplique.

## Estilo de commits

- En **español**, siguiendo [Conventional Commits](https://www.conventionalcommits.org/)
  (`feat:`, `fix:`, `docs:`, `refactor:`, `ci:`…).
- **Un commit por cambio lógico**, no todo junto.

## Traducciones

Los textos viven en `src/lib/i18n.ts`, con el **español como clave**. Para añadir o corregir
un idioma (en, pt, fr) basta con completar su diccionario respetando las mismas claves y las
variables `{n}`.

## Integración continua

Cada Pull Request compila y prueba en **Linux, Windows y macOS** (matriz en `.github/workflows/ci.yml`).
Un PR debe quedar en verde en los tres antes de fusionarse. Además, el job `bench` (Linux) compara
los benchmarks con la línea base versionada y falla si alguno empeora más de un 20 % (ver
«Benchmarks en CI y línea base»).
