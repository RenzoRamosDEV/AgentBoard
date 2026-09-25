# Spec Delta

## ADDED Requirements

### Requirement: Turnos
Cada `promptId` de una sesión SHALL registrarse como un turno con su primera marca de tiempo, y cada llamada y uso de herramienta SHALL asociarse al último turno de su sesión anterior o igual a su marca de tiempo.

#### Scenario: Llamadas de un turno
- **WHEN** un prompt a las 10:00 va seguido de tres respuestas antes del siguiente prompt
- **THEN** las tres llamadas pertenecen a ese turno

### Requirement: Intención del prompt sin guardar texto
El sistema SHALL derivar del texto del prompt una intención (`debug`, `feature`, `brainstorm` o ninguna) por palabras clave en español e inglés y guardar solo esa etiqueta.

#### Scenario: Prompt de corrección
- **WHEN** el prompt es "Fix the failing test"
- **THEN** el turno se guarda con intención `debug` y sin el texto

### Requirement: Detalle de herramientas
El uso de una herramienta SHALL guardar un detalle: el nombre de la skill para `Skill`, el tipo de subagente para `Agent`/`Task` y el `agentId` que devuelve su resultado.

#### Scenario: Subagente lanzado
- **WHEN** se lanza `Agent` con `subagent_type: "Explore"` y su resultado trae `agentId: "a1"`
- **THEN** el uso guarda detalle `Explore` y agente `a1`

### Requirement: Llamadas de subagentes
Las llamadas de líneas con `isSidechain: true` SHALL marcarse como de subagente y guardar su `agentId`.

#### Scenario: Línea de subagente
- **WHEN** se importa una respuesta con `isSidechain: true` y `agentId: "a1"`
- **THEN** la llamada queda asociada al agente `a1`

### Requirement: Relleno del historial
Al añadir estos datos, el sistema SHALL releer los archivos aún presentes para completarlos, sin duplicar llamadas.

#### Scenario: Actualización de la app
- **WHEN** la app se actualiza a esta versión con un historial ya importado
- **THEN** el siguiente escaneo relee los archivos y el número de llamadas no cambia
