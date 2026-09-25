# ingesta-claude-code Specification

## Purpose
Importa el historial de sesiones de Claude Code desde sus archivos JSONL a la base local, de forma incremental, sin duplicados y sin perder datos si los originales se borran.

## Requirements

### Requirement: Detección de la carpeta de logs
El sistema SHALL buscar las sesiones de Claude Code en `<home>/.claude/projects/`, o en `$CLAUDE_CONFIG_DIR/projects/` si esa variable está definida, y registrar el agente solo si la carpeta existe.

#### Scenario: Claude Code instalado
- **WHEN** existe `~/.claude/projects/`
- **THEN** el agente "Claude Code" aparece en la lista de agentes

#### Scenario: Variable de entorno definida
- **WHEN** `CLAUDE_CONFIG_DIR=/opt/claude` y existe `/opt/claude/projects/`
- **THEN** se vigila esa carpeta en lugar de la del home

### Requirement: Conversión de respuestas del modelo en llamadas
Cada línea `type: "assistant"` con `message.usage` SHALL convertirse en una llamada con su `message.id`, sesión, marca de tiempo, modelo y tokens de entrada, salida, lectura de caché y escritura de caché.

#### Scenario: Línea de respuesta válida
- **WHEN** se lee una línea assistant con `usage.input_tokens=2`, `output_tokens=138`, `cache_read_input_tokens=25232`, `cache_creation_input_tokens=13892`
- **THEN** se guarda una llamada con esos cuatro valores

#### Scenario: Línea no reconocida o mal formada
- **WHEN** una línea no es JSON válido o es de un tipo sin uso (por ejemplo `file-history-snapshot`)
- **THEN** se ignora sin detener la importación

### Requirement: Sin duplicados
Una llamada SHALL identificarse por `message.id`; varias líneas con el mismo id MUST producir una sola fila con los tokens de la última.

#### Scenario: Respuesta en streaming
- **WHEN** tres líneas comparten `message.id` con `output_tokens` 10, 50 y 138
- **THEN** existe una sola llamada con `output_tokens=138`

#### Scenario: Reimportar el mismo archivo
- **WHEN** se vuelve a escanear un archivo ya importado
- **THEN** el número de llamadas no cambia

### Requirement: Sesiones y proyectos
Cada `sessionId` SHALL crear una sesión asociada al proyecto de su `cwd` (nombre = última carpeta de la ruta) y guardar rama git, primera y última marca de tiempo y modelo principal.

#### Scenario: Dos sesiones en la misma carpeta
- **WHEN** dos sesiones tienen `cwd` `/home/u/Proyectos/AgentBoard`
- **THEN** ambas pertenecen al proyecto "AgentBoard"

### Requirement: Herramientas usadas
Cada bloque `tool_use` SHALL guardarse como uso de herramienta con su nombre y objetivo (ruta de archivo o comando), y marcarse como error si su `tool_result` llega con `is_error: true`.

#### Scenario: Comando fallido
- **WHEN** un `tool_use` Bash con `command: "npm install"` recibe un `tool_result` con `is_error: true`
- **THEN** existe un uso de herramienta `Bash`, objetivo `npm install`, marcado como error

### Requirement: Lectura incremental
El sistema SHALL recordar por archivo el último byte leído y, en siguientes pasadas, leer solo desde ahí; una línea sin salto final MUST dejarse para la siguiente pasada. Si el archivo encoge o cambia su identificador, se relee desde el principio.

#### Scenario: Archivo con líneas nuevas
- **WHEN** un archivo ya leído hasta el byte 1000 crece a 1500
- **THEN** solo se procesan los bytes 1000–1500

#### Scenario: Archivo truncado
- **WHEN** un archivo leído hasta el byte 1000 pasa a medir 200
- **THEN** se relee desde el byte 0 sin duplicar llamadas

### Requirement: Conservación del historial
Las filas importadas MUST permanecer en la base aunque el archivo original se borre.

#### Scenario: Claude Code borra una sesión antigua
- **WHEN** se elimina un JSONL ya importado y se vuelve a escanear
- **THEN** sus llamadas siguen en la base

### Requirement: Privacidad del texto
El sistema MUST NOT guardar el texto de prompts ni respuestas salvo que el usuario lo active.

#### Scenario: Ajuste por defecto
- **WHEN** se importa una sesión con el ajuste por defecto
- **THEN** la base no contiene el texto de los mensajes

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
