# Spec Delta

## MODIFIED Requirements

### Requirement: Listado de sesiones
El sistema SHALL listar las sesiones con al menos una llamada dentro del filtro activo, con: inicio y fin, agente, proyecto, rama, modelo principal (el de mayor coste), coste, llamadas, cache hit, turnos, compactaciones, usos de herramientas con y sin error y si es una sesión de subagente. Coste, llamadas y cache hit SHALL calcularse con las llamadas del filtro. El listado SHALL entregarse por páginas de como mucho 500 sesiones, ordenado por inicio descendente salvo que se pida otra columna, indicando el total.

#### Scenario: Sesión en el límite del periodo
- **WHEN** una sesión tiene llamadas antes y dentro del periodo
- **THEN** aparece en el listado con el coste de las llamadas del periodo

#### Scenario: Modelo principal
- **WHEN** una sesión gasta 3 USD en claude-opus-4-5 y 1 USD en claude-haiku-4-5
- **THEN** su modelo principal es claude-opus-4-5

#### Scenario: Páginas
- **WHEN** el filtro tiene 120 sesiones y se piden páginas de 50 con desplazamiento 0, 50 y 100
- **THEN** se reciben 50, 50 y 20 sesiones sin repetir ni omitir ninguna, y cada página indica el total de 120

#### Scenario: Orden por otra columna
- **WHEN** se pide la página ordenada por coste descendente
- **THEN** las sesiones llegan de mayor a menor coste en todas las páginas, con desempate estable

#### Scenario: Orden no permitido
- **WHEN** se pide ordenar por una columna que no existe
- **THEN** la petición falla con un error controlado

### Requirement: Detalle de sesión
Al elegir una sesión, el sistema SHALL mostrar sus cifras (coste, turnos, llamadas, cache hit, compactaciones, herramientas y % de error), el coste acumulado a lo largo de la sesión con cada compactación marcada, el coste por actividad, los modelos usados y la comparación del coste por turno antes y después de la tercera compactación (o de la última, si hay menos), sin necesitar la lista completa de turnos.

#### Scenario: Compactaciones
- **WHEN** la sesión tiene dos eventos de compactación
- **THEN** el gráfico de coste acumulado marca los dos momentos

#### Scenario: Coste tras compactar
- **WHEN** los turnos antes de la tercera compactación cuestan 1 USD de media y los de después 3 USD
- **THEN** el detalle indica que tras la compactación 3 el coste por turno es 3 veces el anterior

### Requirement: Turnos de la sesión
El detalle SHALL listar los turnos en orden con hora, actividad, herramientas más usadas, tokens de entrada (incluida la caché) y salida, y coste, cargándolos por páginas con el total de turnos. El texto de los prompts no se guarda ni se muestra. En agentes sin turnos registrados la tabla SHALL indicarlo.

#### Scenario: Agente sin turnos
- **WHEN** la sesión es de un agente que no registra prompts
- **THEN** la tabla de turnos muestra que ese agente no registra turnos

#### Scenario: Sesión con muchos turnos
- **WHEN** una sesión tiene 400 turnos
- **THEN** el detalle muestra los primeros y el total, y el usuario puede cargar los siguientes en orden
