<div align="center">

<img src="src/assets/app-icon.jpg" alt="Logo de AgentBoard" width="120">

# AgentBoard

**El panel de coste y actividad de tus agentes de código, en local.**

Reúne en una sola vista qué hacen Claude Code, Codex, Copilot, Cursor, Gemini y OpenCode
—y cuánto cuestan— leyendo los logs que ya tienen en el ordenador. Nada sale de la máquina.

![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-stable-000000?logo=rust&logoColor=white)
![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=white)
![TypeScript](https://img.shields.io/badge/TypeScript-7-3178C6?logo=typescript&logoColor=white)
![Vite](https://img.shields.io/badge/Vite-8-646CFF?logo=vite&logoColor=white)
![SQLite](https://img.shields.io/badge/SQLite-en%20memoria-003B57?logo=sqlite&logoColor=white)
![MCP](https://img.shields.io/badge/MCP-15%20herramientas-000000)
![CI](https://github.com/RenzoRamosDEV/AgentBoard/actions/workflows/ci.yml/badge.svg)
![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)

</div>

---

## El panel

<div align="center">

<img src="docs/screenshots/panel.png" alt="Resumen de AgentBoard con las tarjetas de coste, sesiones, cache hit, ahorro por caché, burn rate y gasto del mes, y debajo los paneles Daily Activity y By Agent" width="100%">

</div>

| El desglose por herramienta | |
|:---:|:---:|
| <img src="docs/screenshots/tools.png" alt="Tabla de herramientas con llamadas, errores, porcentaje de error y barra de uso por herramienta" width="100%"> | Llamadas y porcentaje de error por herramienta, con su barra de uso. Cada apartado tiene su tabla y su gráfico. |

---

## Qué muestra

Un panel lateral para organizarlo todo (apartados, periodo, agentes y proyectos), y cada
apartado con su tabla y su gráfico, más una vista ampliada al pulsarlo.

| | |
|---|---|
| 📊 | **Resumen** — coste, llamadas, sesiones, cache hit, ahorro por caché, burn rate y gasto del mes con su proyección |
| 📅 | **Daily Activity** — cuánto se gasta cada día |
| 🤖 | **By Agent** — qué agente se usa más y cuánto cuesta cada uno |
| 📁 | **By Project** — coste por proyecto y por rama, con el *overhead* de contexto |
| 🧭 | **By Activity** — coding, testing, debugging, exploración, conversación… |
| 🧠 | **By Model** — coste, cache hit y llamadas por modelo |
| 🔧 | **Tools / Shell Commands** — uso y errores por herramienta y por comando |
| ✨ | **Skills & Agents / MCP Servers / Agent Types** — skills, subagentes y servidores MCP |

Además: **en vivo** (relee los logs al vuelo), **bandeja del sistema** con el gasto del mes,
**avisos de presupuesto** (80 % y 100 %), **exportar** a CSV/JSON, **temas** claro/oscuro/sistema
e **idiomas** (es, en, pt, fr).

---

## Cómo está montado

```
┌──────────────────────────┐        ┌──────────────────────────┐
│  Interfaz (React + TS)   │        │  Agente de IA            │
│  panel, tablas, gráficos │        │  (Claude Code, Codex…)   │
└────────────┬─────────────┘        └────────────┬─────────────┘
             │ comandos Tauri                     │ MCP (stdio)
             ▼                                     ▼
        ┌─────────────────────────────────────────────────┐
        │  Núcleo en Rust                                  │
        │  providers · ingesta · consultas · precios       │
        └────────────────────────┬────────────────────────┘
                                 │ escanea (solo lectura)
                                 ▼
              SQLite en memoria  ◀── logs de los agentes en disco
```

- **Solo lectura y en memoria.** Lee los logs existentes a una base SQLite en memoria; no
  escribe datos de uso en disco (solo el archivo de ajustes).
- **Un módulo por agente** (`providers/`), ingesta incremental por offsets (nunca duplica) y
  consultas agregadas para el panel.
- **Servidor MCP** aparte (`agentboard-mcp`) que expone lo mismo a cualquier agente.

Detalle completo en la [wiki](docs/wiki/Home.md) y en [`CONTRIBUTING.md`](CONTRIBUTING.md).
La especificación se gestiona con [OpenSpec](https://github.com/Fission-AI/OpenSpec) en `openspec/`.

---

## De dónde salen los datos

| Agente | Origen |
| --- | --- |
| Claude Code | `~/.claude/projects/**/*.jsonl` (o `$CLAUDE_CONFIG_DIR`) |
| Codex CLI | `~/.codex/sessions/**/rollout-*.jsonl` (o `$CODEX_HOME`) |
| GitHub Copilot CLI | `~/.copilot/session-state/*/events.jsonl` (o `$COPILOT_HOME`) |
| Cursor CLI | `~/.cursor/projects/*/agent-transcripts/*/*.jsonl` |
| Gemini CLI | `~/.gemini/tmp/*/chats/session-*.jsonl` (o `$GEMINI_CLI_HOME`) |
| OpenCode | base SQLite de OpenCode (lectura incremental) |

Un agente aparece si está instalado, aunque aún no tenga sesiones. Cada llamada se identifica
por su `message_id`, así que releer nunca duplica. El texto de prompts y respuestas no se guarda.

---

## Servidor MCP

`agentboard-mcp` expone los datos de uso a cualquier agente por Model Context Protocol (stdio),
sin necesitar la app abierta.

```bash
cd src-tauri && cargo build --release --bin agentboard-mcp
claude mcp add agentboard -- $(pwd)/target/release/agentboard-mcp
```

15 herramientas (`get_summary`, `get_cost_by_model`, `get_daily`, `list_agents`…), todas con
filtro opcional de `period`, `agents` y `projects`. Guía completa en la
[wiki del MCP](docs/wiki/Servidor-MCP.md).

---

## Instalación

Descarga el instalador de tu sistema desde
[**Releases**](https://github.com/RenzoRamosDEV/AgentBoard/releases):

- **Linux** — `.AppImage` o `.deb`
- **Windows** — `.msi` o `.exe`
- **macOS** — `.dmg` (universal, Intel + Apple Silicon)

> [!NOTE]
> Los binarios de Windows y macOS van sin firmar; la primera vez el sistema pedirá confirmación
> (SmartScreen / Gatekeeper).

---

## Desarrollo

Requisitos: Node.js 20+, Rust estable y, en Linux, WebKitGTK 4.1.

```bash
npm ci
npm run tauri dev     # app con recarga en caliente
npm test              # tests del frontend (vitest)
```

Backend:

```bash
cargo test  --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml --bin agentboard-mcp
```

Cada Pull Request compila y prueba en **Linux, Windows y macOS**. Ver [`CONTRIBUTING.md`](CONTRIBUTING.md).

---

## Licencia

[MIT](LICENSE) © Renzo Ramos
