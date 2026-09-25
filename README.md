# AgentBoard

App de escritorio (Linux, Windows y macOS) que muestra qué hacen tus agentes de código —Claude Code, Codex, Gemini CLI— y cuánto cuestan, y guarda ese historial en una base SQLite local para siempre. Sin proxy, sin API keys y sin enviar datos fuera de tu máquina.

- **Stack:** Tauri 2 · Rust · SQLite (WAL) · React + TypeScript + Vite
- **Especificación:** gestionada con [OpenSpec](https://github.com/Fission-AI/OpenSpec) en `openspec/` (specs vigentes en `openspec/specs/`, cambios en `openspec/changes/`).

## Requisitos

| Herramienta | Versión |
| --- | --- |
| Node.js | 20+ |
| Rust | stable (rustup) |
| Linux | WebKitGTK 4.1, libappindicator, librsvg ([guía de Tauri](https://v2.tauri.app/start/prerequisites/)) |

### Fedora / Bazzite (distrobox)

En distros inmutables se compila dentro de un contenedor que comparte tu `$HOME`:

```sh
distrobox create -n agentboard-dev -i registry.fedoraproject.org/fedora-toolbox:44
distrobox enter agentboard-dev -- sudo dnf install -y webkit2gtk4.1-devel openssl-devel \
  libappindicator-gtk3-devel librsvg2-devel libxdo-devel gcc gcc-c++ make
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
```

## Desarrollo

```sh
npm install
npm run tauri dev          # en Bazzite: distrobox enter agentboard-dev -- npm run tauri dev
```

Tests del núcleo y verificación con tus datos reales:

```sh
cd src-tauri
cargo test                 # parsers, ingesta incremental, precios y consultas
cargo run --example scan   # importa tu historial a una base temporal e imprime el resumen
```

## Dónde guarda los datos

| Sistema | Base de datos |
| --- | --- |
| Linux | `~/.local/share/agentboard/agentboard.db` |
| Windows | `%APPDATA%\agentboard\agentboard.db` |
| macOS | `~/Library/Application Support/agentboard/agentboard.db` |

Los logs se leen de `~/.claude/projects/` (o `$CLAUDE_CONFIG_DIR/projects/`). Cada archivo se lee una vez y después solo sus líneas nuevas; cada llamada se guarda por su `message_id`, así que releer nunca duplica y borrar el original nunca quita datos. El texto de prompts y respuestas no se guarda.

## Estructura

```
src/                  UI React
src-tauri/src/
  db.rs               conexión SQLite y migraciones
  ingest.rs           offsets por archivo, upsert, eventos
  pricing.rs          precios por modelo (USD / millón de tokens)
  queries.rs          consultas agregadas para la UI
  commands.rs         comandos Tauri
  providers/          un módulo por agente (trait Provider)
src-tauri/migrations/ esquema versionado
tests/fixtures/       JSONL anonimizados por agente
```
