# ingesta-opencode Specification

## Purpose
Importa el historial de OpenCode desde su base SQLite local, de forma incremental y sin duplicados, con los mismos conceptos (sesión, turno, llamada, herramienta, subagente) que el resto de agentes.

## Requirements

### Requirement: Detección de la base de OpenCode
El sistema SHALL buscar `opencode.db` en la carpeta de datos de OpenCode (`~/.local/share/opencode/` en Linux, `%APPDATA%\opencode\` en Windows, `~/Library/Application Support/opencode/` en macOS) y registrar el agente "OpenCode" si existe.

#### Scenario: OpenCode instalado
- **WHEN** existe `~/.local/share/opencode/opencode.db`
- **THEN** "OpenCode" aparece en la lista de agentes

### Requirement: Lectura en solo lectura e incremental
La base SHALL abrirse en solo lectura y en cada pasada SHALL leerse solo lo modificado desde el último `time_updated` procesado.

#### Scenario: Segunda pasada sin cambios
- **WHEN** se vuelve a escanear sin que OpenCode haya escrito nada
- **THEN** no se procesa ningún mensaje y el número de llamadas no cambia

### Requirement: Conversión de mensajes
Cada mensaje `assistant` SHALL convertirse en una llamada con su modelo (`proveedor/modelo`), tokens de entrada, salida, razonamiento, lectura y escritura de caché, y el coste reportado; cada mensaje `user` SHALL abrir un turno; cada parte `tool` SHALL guardarse como uso de herramienta con nombre normalizado, objetivo, error y duración.

#### Scenario: Mensaje del asistente
- **WHEN** un mensaje assistant trae `tokens.input=6497`, `output=168`, `cache.read=1792` y `modelID=big-pickle`
- **THEN** existe una llamada con esos tokens y modelo `big-pickle`

#### Scenario: Herramienta con error
- **WHEN** una parte `tool` `bash` tiene `state.status = "error"`
- **THEN** el uso de herramienta `Bash` queda marcado como error

### Requirement: Sesiones hijas como subagentes
Las sesiones con `parent_id` SHALL marcarse como subagente y sus llamadas MUST contarse como de subagente.

#### Scenario: Sesión hija
- **WHEN** una sesión tiene `parent_id` no nulo
- **THEN** sus llamadas aparecen en "Claude Agent Types" y no en el modelo principal de la sesión padre
