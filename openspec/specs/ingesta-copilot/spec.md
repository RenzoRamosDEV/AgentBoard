# ingesta-copilot Specification

## Purpose
Importa las sesiones de GitHub Copilot CLI desde su flujo de eventos local con los mismos conceptos (sesión, turno, llamada, herramienta) que el resto de agentes.

## Requirements

### Requirement: Detección de sesiones de Copilot
El sistema SHALL leer `<COPILOT_HOME o ~/.copilot>/session-state/*/events.jsonl` y registrar el agente "GitHub Copilot CLI" si la carpeta existe.

#### Scenario: Copilot con sesiones
- **WHEN** existe `~/.copilot/session-state/<id>/events.jsonl`
- **THEN** "GitHub Copilot CLI" aparece en la lista de agentes con esa sesión

### Requirement: Llamadas y tokens
Cada `assistant.message` SHALL ser una llamada con su modelo y tokens de salida; en cada `session.shutdown` el sistema SHALL calcular por modelo el delta de tokens respecto al cierre anterior y repartirlo entre las respuestas de ese modelo registradas desde entonces, de modo que la suma de tokens de la sesión coincida con los totales del cierre.

#### Scenario: Sesión cerrada
- **WHEN** una sesión tiene dos respuestas de `claude-sonnet-4.5` y su cierre reporta 3000 tokens de entrada para ese modelo
- **THEN** ambas llamadas suman 3000 de entrada y el modelo se guarda como `claude-sonnet-4-5`

#### Scenario: Sesión reanudada
- **WHEN** una sesión reanudada cierra de nuevo con totales mayores
- **THEN** solo la diferencia se asigna a las respuestas posteriores al primer cierre

### Requirement: Turnos y herramientas
Cada `assistant.turn_start` SHALL abrir un turno cuya intención sale del `user.message` anterior; `tool.execution_start` y `tool.execution_complete` SHALL guardarse como uso de herramienta con nombre normalizado, objetivo, error (`success: false`) y duración.

#### Scenario: Herramienta fallida
- **WHEN** `bash` termina con `success: false` un segundo después de empezar
- **THEN** existe un uso `Bash` con error y 1000 ms de duración
