# AgentBoard

AgentBoard es una aplicación de escritorio de **código abierto** que muestra qué hacen los
agentes de código de una persona —Claude Code, Codex CLI, GitHub Copilot CLI, Cursor CLI,
Gemini CLI y OpenCode— y cuánto cuestan, leyendo los registros que esos agentes ya dejan en
el ordenador.

El proyecto nace de una idea sencilla: los agentes de código generan mucha actividad (llamadas,
tokens, sesiones, herramientas, comandos) y un gasto que normalmente queda repartido y opaco.
AgentBoard lo reúne en un único panel, sin enviar nada fuera de la máquina.

## Principios

- **Local y privado.** No usa proxy, ni claves de API, ni telemetría. Ningún dato sale del equipo.
- **Solo lectura.** Lee los logs existentes; no modifica el trabajo de los agentes.
- **Sin persistencia.** Los datos de uso viven en una base SQLite **en memoria** mientras la app
  está abierta. Lo único que se guarda en disco es el archivo de ajustes.
- **Multiplataforma.** Se compila y ejecuta en Linux, Windows y macOS (Tauri + Rust + React).

## Licencia

AgentBoard se distribuye bajo licencia **MIT**. El código está disponible para leerlo, usarlo,
modificarlo y redistribuirlo.

## Páginas de esta wiki

- **[[Funcionalidad]]** — qué muestra la app y sus funciones de escritorio.
- **[[Arquitectura]]** — cómo se montó el proyecto por dentro.
- **[[Servidor MCP|Servidor-MCP]]** — cómo cualquier agente puede consultar los datos por MCP.
