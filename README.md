# AgentBoard

App de escritorio (Linux, Windows y macOS) que muestra qué hacen tus agentes de código —Claude Code, Codex CLI, GitHub Copilot CLI, Cursor CLI, Gemini CLI, OpenCode— y cuánto cuestan, leyendo los logs que ya tienes en el ordenador. No guarda nada en disco (los datos viven en memoria mientras la app está abierta; solo el presupuesto de Ajustes se escribe en `~/.config/agentboard/settings.json`). Sin proxy, sin API keys y sin enviar datos fuera de tu máquina.

- **Stack:** Tauri 2 · Rust · SQLite (WAL) · React + TypeScript + Vite
- **Especificación:** gestionada con [OpenSpec](https://github.com/Fission-AI/OpenSpec) en `openspec/` (specs vigentes en `openspec/specs/`, cambios en `openspec/changes/`).

## Qué muestra

Panel izquierdo para organizarlo todo: apartados, periodo (Hoy, 7 días, 30 días, Mes, 6 meses, Todo, rango), agentes y proyectos detectados, datos y ajustes (presupuesto mensual). Cada apartado tiene su tabla y su gráfico, y una vista ampliada al pulsarlo:

| Apartado | Tabla | Gráfico |
| --- | --- | --- |
| Resumen | coste, sesiones, cache hit, ahorro por caché, burn rate, gasto del mes y presupuesto | — |
| Daily Activity | coste y llamadas por día | columnas por día |
| By Project | coste, media por sesión, sesiones y overhead de contexto (por rama con un proyecto elegido) | reparto del coste |
| By Activity | Coding, Exploration, Testing, Delegation, Conversation, Build/Deploy, Feature Dev, Debugging, Brainstorming, General: coste, turnos y 1-shot | anillo de reparto; ampliada: apilado por día y 1-shot |
| By Model | coste, cache hit, llamadas y 1-shot | coste por modelo |
| Tools | llamadas y % de error por herramienta | uso |
| Shell Commands | comandos por primera palabra (separando tuberías) | uso |
| Skills & Agents | usos y coste de skills y subagentes invocados | coste |
| MCP Servers | llamadas por servidor MCP | uso |
| Claude Agent Types | llamadas y coste dentro de subagentes por tipo | coste |

**Ajustes**: tema (sistema, claro u oscuro), idioma (sistema, español, inglés, portugués o francés) y presupuesto mensual. Las traducciones viven en `src/lib/i18n.ts`, con el texto en español como clave.

**Funciones de escritorio**
- **En vivo**: un vigilante (`notify`) relee los logs al vuelo mientras la app está abierta; el dashboard se actualiza sin recargar.
- **Bandeja del sistema**: icono con el gasto del mes en el tooltip y menú Mostrar / Salir; al cerrar la ventana la app sigue en segundo plano.
- **Avisos de presupuesto**: notificación nativa al llegar al 80 % y al 100 % de la proyección del mes.
- **Exportar**: las llamadas del filtro activo a CSV o JSON desde el panel lateral.

El diseño está en Claude Design: <https://claude.ai/artifact/5gfYnEQqfmuqPJu7xaunqc>.

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
cargo run --example scan   # lee tu historial en memoria e imprime cada apartado
```

## De dónde salen los datos

Agentes soportados y de dónde se leen:

| Agente | Origen |
| --- | --- |
| Claude Code | `~/.claude/projects/**/*.jsonl` (o `$CLAUDE_CONFIG_DIR/projects/`) |
| Codex CLI | `~/.codex/sessions/**/rollout-*.jsonl` y `archived_sessions/` (o `$CODEX_HOME`) |
| GitHub Copilot CLI | `~/.copilot/session-state/*/events.jsonl` (o `$COPILOT_HOME`) |
| Cursor CLI | `~/.cursor/projects/*/agent-transcripts/*/*.jsonl` (tokens estimados: Cursor no los apunta) |
| Gemini CLI | `~/.gemini/tmp/*/chats/session-*.jsonl` (o `$GEMINI_CLI_HOME`) |
| OpenCode | `~/.local/share/opencode/opencode.db` (SQLite, lectura incremental) |

Un agente aparece en la lista si está instalado (ejecutable en el PATH o su carpeta de configuración) aunque aún no tenga sesiones; sus datos salen en cuanto existan. Al arrancar se leen todos los logs a una base SQLite en memoria; cada llamada se identifica por su `message_id`, así que releer nunca duplica. El texto de prompts y respuestas no se guarda en ningún sitio.

## Estructura

```
src/                  UI React (views/ = resumen y vista ampliada; components/ = panel lateral, tablas, gráficos)
src-tauri/src/
  db.rs               base SQLite en memoria y migraciones
  ingest.rs           offsets por archivo, upsert, eventos
  pricing.rs          precios por modelo (USD / millón de tokens)
  queries.rs          consultas agregadas para la UI
  commands.rs         comandos Tauri
  providers/          un módulo por agente (trait Provider)
src-tauri/migrations/ esquema versionado
tests/fixtures/       JSONL anonimizados por agente
```
