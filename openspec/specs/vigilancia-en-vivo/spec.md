# vigilancia-en-vivo Specification

## Purpose
Mantiene el dashboard al día releyendo los logs de los agentes mientras la app está abierta, sin que el usuario recargue ni reinicie.

## Requirements

### Requirement: Relectura automática al cambiar los logs
El sistema SHALL observar las carpetas de logs de los agentes detectados y, cuando un archivo cambie, releer de forma incremental y avisar a la interfaz para que refresque sus vistas.

#### Scenario: Nueva actividad con la app abierta
- **WHEN** un agente escribe una llamada nueva en su log mientras AgentBoard está abierto
- **THEN** en menos de 2 segundos las vistas reflejan esa llamada sin recargar

#### Scenario: Sin duplicados
- **WHEN** el vigilante relee un archivo que ya había importado
- **THEN** las llamadas ya guardadas no se duplican

### Requirement: Agrupación de eventos
El sistema SHALL agrupar los cambios seguidos (debounce) antes de releer, para no releer en cada byte escrito.

#### Scenario: Ráfaga de escrituras
- **WHEN** un archivo recibe varias escrituras en menos de medio segundo
- **THEN** se produce una sola relectura tras la ráfaga

### Requirement: Carpetas que aparecen después
Si la carpeta de logs de un agente no existía al arrancar y se crea después, el sistema SHALL empezar a observarla sin reiniciar la app.

#### Scenario: Agente instalado con la app abierta
- **WHEN** se usa por primera vez un agente cuya carpeta de logs no existía
- **THEN** su actividad aparece sin reiniciar AgentBoard
