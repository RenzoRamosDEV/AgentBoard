# dashboard Specification

## Purpose
Responde de un vistazo cuánto se gasta, a qué ritmo y en qué (días, proyectos, actividades, modelos, herramientas, skills, servidores MCP y subagentes), siempre respetando los filtros activos, con la organización en apartados de CodeBurn.

## Requirements

### Requirement: Cabecera de resumen
El dashboard SHALL mostrar arriba, para el filtro activo: nombre del periodo, coste total (USD), llamadas, sesiones, cache hit (`cache_read / (input + cache_read + cache_write)`), tokens de entrada, salida, leídos de caché y escritos en caché, ahorro por caché (`cache_read × (precio entrada − precio lectura caché)`) y burn rate (coste de los últimos 60 minutos, USD/h).

#### Scenario: Cache hit
- **WHEN** en el periodo hay 100 tokens de entrada, 800 de lectura de caché y 100 de escritura
- **THEN** el cache hit mostrado es 80 %

#### Scenario: Burn rate sin actividad reciente
- **WHEN** no hay llamadas en los últimos 60 minutos
- **THEN** el burn rate es 0 USD/h

### Requirement: Gasto del mes y presupuesto
La cabecera SHALL mostrar el coste acumulado del mes en curso y la proyección a fin de mes (`acumulado / días transcurridos × días del mes`), ignorando el periodo pero respetando agentes y proyectos; si hay presupuesto mensual, SHALL mostrar el porcentaje consumido y avisar cuando la proyección lo supere.

#### Scenario: Proyección
- **WHEN** el día 10 de un mes de 30 días el acumulado es 20 USD
- **THEN** la proyección es 60 USD

#### Scenario: Presupuesto superado
- **WHEN** el presupuesto es 50 USD y la proyección 60 USD
- **THEN** se muestra un aviso de que la proyección supera el presupuesto

### Requirement: Daily Activity
El apartado SHALL listar cada día local con actividad del periodo, con su coste, llamadas y una barra proporcional al coste.

#### Scenario: Día sin actividad
- **WHEN** un día del periodo no tiene llamadas
- **THEN** ese día no aparece en la lista

### Requirement: By Project
El apartado SHALL mostrar por proyecto: coste, coste medio por sesión, sesiones y overhead (media de tokens de contexto de la primera llamada de cada sesión), ordenado por coste; con exactamente un proyecto seleccionado SHALL mostrar las mismas columnas por rama git.

#### Scenario: Coste medio por sesión
- **WHEN** un proyecto tiene 3 sesiones y 30 USD
- **THEN** su columna avg/s muestra 10 USD

#### Scenario: Un proyecto elegido
- **WHEN** el filtro incluye solo el proyecto "web"
- **THEN** el apartado lista las ramas de "web"

### Requirement: By Activity
Cada turno (un prompt del usuario y todo lo que el agente hace hasta el siguiente) SHALL clasificarse en una actividad —Coding, Feature Dev, Debugging, Testing, Build/Deploy, Git, Exploration, Delegation, Brainstorming o Conversation— según sus herramientas y palabras clave del prompt. El apartado SHALL mostrar por actividad: coste, turnos y 1-shot en las actividades con ediciones.

#### Scenario: Turno de depuración
- **WHEN** el prompt contiene "arregla el error" y el turno edita archivos
- **THEN** el turno cuenta como Debugging

#### Scenario: Turno sin herramientas
- **WHEN** un turno solo tiene respuestas de texto
- **THEN** cuenta como Conversation, o Brainstorming si el prompt pide ideas u opinión

### Requirement: 1-shot
El 1-shot SHALL ser el porcentaje de turnos con ediciones en los que ninguna edición falló y ningún archivo se editó más de una vez.

#### Scenario: Reedición
- **WHEN** un turno edita `a.rs`, falla un test y vuelve a editar `a.rs`
- **THEN** ese turno no es 1-shot

### Requirement: By Model
El apartado SHALL mostrar por modelo, con su nombre legible (p. ej. "Opus 5.5"): coste, cache hit, llamadas y 1-shot, y marcar los modelos sin precio conocido.

#### Scenario: Modelo sin precio
- **WHEN** hay llamadas de un modelo sin precio
- **THEN** aparece con coste 0 y la marca "sin precio"

### Requirement: Core Tools y Shell Commands
Core Tools SHALL listar las herramientas nativas (excluidas las MCP) por número de usos. Shell Commands SHALL contar cada comando de una línea de shell por su primera palabra, separando tuberías y encadenamientos (`|`, `&&`, `||`, `;`).

#### Scenario: Tubería
- **WHEN** se ejecutó `grep -r foo . | head -5 && git status`
- **THEN** cuentan un uso de `grep`, uno de `head` y uno de `git`

### Requirement: Skills & Agents
El apartado SHALL listar cada skill invocada y cada tipo de subagente lanzado, con sus usos y el coste de las respuestas del modelo que los invocaron.

#### Scenario: Skill invocada
- **WHEN** la respuesta que invoca la skill `dataviz` cuesta 0,20 USD
- **THEN** `dataviz` aparece con 1 uso y 0,20 USD

### Requirement: MCP Servers
El apartado SHALL listar cada servidor MCP (segundo segmento de `mcp__<servidor>__<herramienta>`) con su número de llamadas.

#### Scenario: Herramienta MCP
- **WHEN** se llamó a `mcp__claude_ai_Slack__slack_send_message`
- **THEN** `claude_ai_Slack` suma una llamada

### Requirement: Claude Agent Types
El apartado SHALL agrupar las llamadas hechas dentro de subagentes por tipo de subagente, con llamadas y coste.

#### Scenario: Subagente Explore
- **WHEN** un subagente de tipo `Explore` hace 82 llamadas
- **THEN** `Explore` aparece con 82 llamadas y su coste

### Requirement: Actualización tras la importación
Los apartados SHALL refrescarse cuando termine una importación.

#### Scenario: Escaneo inicial termina
- **WHEN** el escaneo inicial acaba mientras el dashboard está abierto
- **THEN** las cifras se actualizan sin recargar la app
