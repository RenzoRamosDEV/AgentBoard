# Tasks

## 1. Andamiaje

- [x] 1.1 Crear el proyecto Vite + React + TypeScript en la raíz y verificar que `npm run build` genera `dist/`
- [x] 1.2 Añadir `src-tauri/` con Tauri 2, `tauri.conf.json` e iconos, y verificar que `cargo check` pasa dentro del distrobox
- [x] 1.3 Documentar en README.md cómo instalar dependencias y ejecutar `npm run tauri dev` (incluido el distrobox en Bazzite)

## 2. Base de datos

- [x] 2.1 Implementar `db.rs` (ruta de datos con `dirs`, WAL, migraciones por `user_version`) y verificar con un test que aplicar dos veces no falla
- [x] 2.2 Escribir `migrations/0001_inicial.sql` con las ocho tablas, índices y la vista `call_costs`, y verificar con un test que la vista existe
- [x] 2.3 Implementar `pricing.rs` con la tabla inicial de precios y verificar con un test el ejemplo de 4,5 USD y el modelo sin precio a 0

## 3. Ingesta de Claude Code

- [x] 3.1 Definir el trait `Provider` y los tipos `Record` en `providers/mod.rs` y verificar que compila
- [x] 3.2 Implementar `providers/claude_code.rs` y verificar con fixtures que se extraen llamadas, tool calls, errores, sesión, cwd y rama
- [x] 3.3 Implementar `ingest.rs` (file_state, offsets, línea incompleta, truncado, upsert) y verificar con tests de reimportación, crecimiento y truncado
- [x] 3.4 Añadir fixtures anonimizados en `tests/fixtures/claude_code/` y verificar que `cargo test` pasa

## 4. Integración mínima

- [x] 4.1 Exponer el comando `get_summary` y lanzar el escaneo inicial al arrancar; verificar que devuelve coste, llamadas y sesiones
- [x] 4.2 Pantalla mínima en React que muestra el resumen y verificar con `npm run tauri dev` que aparecen los datos reales de esta máquina
