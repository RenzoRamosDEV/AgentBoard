## MODIFIED Requirements

### Requirement: Listado de sesiones
El sistema SHALL listar las sesiones con alguna llamada en el filtro activo, con: inicio y fin, agente, proyecto, rama, modelo principal (el de mayor coste), coste, llamadas, cache hit, turnos, compactaciones, usos de herramientas con y sin error y si es una sesión de subagente. Coste, llamadas y cache hit SHALL calcularse con las llamadas del filtro. El listado SHALL ir paginado (máximo 500 por página) con el total, ordenado por inicio descendente salvo que se pida otra columna.

#### Scenario: Sesión en el límite del periodo
- **WHEN** una sesión tiene llamadas antes y dentro del periodo
- **THEN** aparece en el listado con el coste de las llamadas del periodo

#### Scenario: Modelo principal
- **WHEN** una sesión gasta 3 USD en claude-opus-4-5 y 1 USD en claude-haiku-4-5
- **THEN** su modelo principal es claude-opus-4-5

#### Scenario: Páginas
- **WHEN** el filtro tiene 120 sesiones y se piden páginas de 50 con desplazamiento 0, 50 y 100
- **THEN** se reciben 50, 50 y 20 sesiones sin repetir ni omitir ninguna, y cada página indica el total de 120

#### Scenario: Tope de página
- **WHEN** se pide una página de más de 500 sesiones
- **THEN** la página trae como mucho 500 sesiones

#### Scenario: Orden por defecto
- **WHEN** se pide una página sin indicar columna de orden
- **THEN** las sesiones llegan de inicio más reciente a más antiguo

#### Scenario: Orden por otra columna
- **WHEN** se pide la página ordenada por coste descendente
- **THEN** las sesiones llegan de mayor a menor coste en todas las páginas, con desempate estable

#### Scenario: Orden no permitido
- **WHEN** se pide ordenar por una columna que no existe
- **THEN** la petición falla con un error controlado
