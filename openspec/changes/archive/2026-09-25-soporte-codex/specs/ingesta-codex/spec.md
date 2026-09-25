# Spec Delta

## Purpose

Importa las sesiones de Codex CLI desde sus archivos rollout JSONL con los mismos conceptos (sesión, turno, llamada, herramienta, subagente) que el resto de agentes.

## ADDED Requirements

### Requirement: Detección de sesiones de Codex
El sistema SHALL leer los archivos `rollout-*.jsonl` bajo `<CODEX_HOME o ~/.codex>/sessions/` y `archived_sessions/`, y registrar el agente "Codex CLI" si alguna carpeta existe.

#### Scenario: Codex instalado con sesiones
- **WHEN** existe `~/.codex/sessions/2026/09/24/rollout-….jsonl`
- **THEN** "Codex CLI" aparece en la lista de agentes con esa sesión

### Requirement: Una llamada por respuesta
Cada línea `token_usage_record` SHALL convertirse en una llamada identificada por `response_id`, con el modelo del último `turn_context`, entrada sin la parte cacheada, salida, razonamiento y lectura de caché. Los `event_msg` `token_count` MUST ignorarse cuando el archivo tiene registros de uso.

#### Scenario: Respuesta con caché
- **WHEN** un registro trae `input_tokens=12000`, `cached_input_tokens=9000`, `output_tokens=300`
- **THEN** la llamada guarda 3000 de entrada, 9000 de caché y 300 de salida

### Requirement: Turnos, herramientas y eventos
Cada `turn_context` SHALL abrir un turno cuya intención sale del `user_message` del mismo turno; las llamadas a herramienta (`function_call`, `custom_tool_call`, `local_shell_call`) SHALL guardarse con nombre normalizado, objetivo (comando o archivo del parche) y error según `success` o `exit_code`; `compacted` y `turn_aborted` SHALL guardarse como eventos.

#### Scenario: Parche aplicado
- **WHEN** un `custom_tool_call` `apply_patch` actualiza `src/pricing.rs` y su salida trae `success: true`
- **THEN** existe un uso `Edit` con objetivo `src/pricing.rs` sin error

### Requirement: Subagentes
Una sesión con `parent_thread_id` o `thread_source: subagent` SHALL marcarse como subagente y sus llamadas MUST contarse como de subagente.

#### Scenario: Hilo hijo
- **WHEN** un rollout tiene `parent_thread_id`
- **THEN** sus llamadas aparecen en "Claude Agent Types" y no abren turnos
