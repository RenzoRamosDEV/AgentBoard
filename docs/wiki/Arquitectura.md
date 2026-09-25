# Arquitectura — cómo se montó

AgentBoard se construyó con **Tauri 2**: un backend en **Rust** que hace todo el trabajo de
lectura y agregación, y una interfaz en **React + TypeScript + Vite**. El resultado es un único
binario nativo para Linux, Windows y macOS.

## Backend (Rust, `src-tauri/`)

- **Base en memoria.** Al arrancar se abre una base **SQLite en memoria** y se leen los logs de
  todos los agentes detectados. No se escribe nada en disco (salvo los ajustes). El esquema se
  versiona con migraciones (`PRAGMA user_version`).
- **Providers.** Cada agente es un módulo en `providers/` que implementa el trait `Provider`:
  detecta si el agente está instalado, dice dónde están sus logs y parsea sus registros a unos
  tipos comunes (sesiones, llamadas, turnos, usos de herramienta). Las fuentes pueden ser JSONL
  (Claude Code, Codex, Copilot, Cursor, Gemini) o SQLite (OpenCode).
- **Ingesta incremental.** `ingest.rs` recuerda por qué offset va cada archivo, así que releer
  solo procesa lo nuevo; cada llamada se identifica por su `message_id` para no duplicar.
- **Consultas.** `queries.rs` resuelve las cifras del panel con agregaciones SQL (por agente,
  modelo, proyecto, actividad, herramienta…), siempre respetando el filtro común.
- **Precios.** `pricing.rs` guarda el precio por modelo (USD por millón de tokens) y calcula
  coste y ahorro por caché. Los modelos sin precio conocido cuentan como 0.
- **Vigilante en vivo.** `watcher.rs` usa el crate `notify` para observar las carpetas de logs y
  releer al vuelo cuando algo cambia, agrupando ráfagas de escrituras (debounce).

## Frontend (React + TypeScript, `src/`)

- **Vistas y componentes.** `views/` arma el resumen y las vistas ampliadas; `components/`
  contiene el panel lateral, las tablas y los gráficos, dibujados a medida.
- **Internacionalización.** `lib/i18n.ts` usa el **texto en español como clave** y diccionarios
  para inglés, portugués y francés.
- **Temas.** El tema (claro, oscuro o del sistema) se resuelve por un atributo en el documento;
  el modo oscuro usa un acento naranja y el claro uno celeste.

## Especificación primero

El proyecto se desarrolla con **OpenSpec**: cada cambio se describe primero como propuesta y
especificación en `openspec/` antes de implementarse, y se archiva al completarse.

## Integración continua y distribución

- Cada Pull Request compila y prueba la app y el servidor MCP en **Linux, Windows y macOS**
  (matriz en GitHub Actions), además de comprobar formato, clippy, tipos y tests.
- Un tag `v*` dispara el workflow de release, que empaqueta los instaladores de cada sistema
  (`.AppImage`/`.deb`, `.msi`/`.exe`, `.dmg` universal) con `tauri-action`.
