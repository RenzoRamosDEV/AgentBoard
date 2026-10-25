# vigilancia-en-vivo Specification

## Purpose
Mantiene el dashboard al día releyendo los logs de los agentes mientras la app está abierta, sin que el usuario recargue ni reinicie.

## Requirements

### Requirement: Relectura automática al cambiar los logs
El sistema SHALL observar las carpetas de logs de los agentes detectados y, cuando un archivo cambie, releer de forma incremental solo los archivos cambiados (sin recorrer el resto de carpetas) y avisar a la interfaz para que refresque sus vistas.

#### Scenario: Nueva actividad con la app abierta
- **WHEN** un agente escribe una llamada nueva en su log mientras AgentBoard está abierto y no hubo otro aviso en el último segundo
- **THEN** en menos de 500 ms desde el evento del sistema de archivos los datos que lee la interfaz incluyen esa llamada y la interfaz ha recibido el aviso

#### Scenario: Sin duplicados
- **WHEN** el vigilante relee un archivo que ya había importado
- **THEN** las llamadas ya guardadas no se duplican

#### Scenario: Solo el archivo tocado
- **WHEN** un agente añade líneas a un log entre miles de archivos ya importados
- **THEN** la relectura abre solo ese archivo y no consulta el estado en disco de los demás

#### Scenario: Log borrado
- **WHEN** se borra un log ya importado con la app abierta
- **THEN** sus datos se retiran como en un escaneo completo y la interfaz recibe el aviso

### Requirement: Agrupación de eventos
El sistema SHALL agrupar por archivo los cambios seguidos (debounce de 150 a 300 ms desde el último evento de ese archivo) antes de releerlo, con una espera máxima de 1 s para un archivo que no deja de cambiar, y MUST ignorar los eventos de solo acceso y los de rutas que ningún agente reconoce como log.

#### Scenario: Ráfaga de escrituras
- **WHEN** un archivo recibe varias escrituras en menos de medio segundo
- **THEN** se produce una sola relectura de ese archivo tras la ráfaga

#### Scenario: Archivo que no para de crecer
- **WHEN** un agente escribe en su log cada 100 ms durante 5 s
- **THEN** el archivo se relee al menos una vez por segundo mientras dura la escritura

#### Scenario: Eventos irrelevantes
- **WHEN** en una carpeta vigilada se lee un log o se crea un archivo que ningún agente reconoce
- **THEN** no se relee nada ni se avisa a la interfaz

### Requirement: Carpetas que aparecen después
Si la carpeta de logs de un agente no existía al arrancar y se crea después, el sistema SHALL empezar a observarla sin reiniciar la app, detectándola por eventos de una carpeta antepasada existente y sin revisiones periódicas, e importar los logs que ya contenga.

#### Scenario: Agente instalado con la app abierta
- **WHEN** se usa por primera vez un agente cuya carpeta de logs no existía
- **THEN** su actividad aparece sin reiniciar AgentBoard

#### Scenario: Carpeta creada con logs ya escritos
- **WHEN** se crea la carpeta de logs de un agente y sus archivos se escriben antes de que el vigilante la observe
- **THEN** esos archivos se importan igualmente

### Requirement: Agentes con estado entre líneas
La relectura en vivo SHALL funcionar igual para todos los agentes, incluidos aquellos cuyo log solo identifica la sesión en sus primeras líneas (Codex, Gemini CLI, Cursor, Copilot) y los que guardan sus datos en una base SQLite (OpenCode, cuyos cambios llegan por sus archivos auxiliares): si el sistema no conserva el estado de lectura de un archivo, SHALL releerlo desde el principio sin duplicar datos.

#### Scenario: Codex con la app abierta
- **WHEN** se inicia una sesión de Codex CLI con AgentBoard ya abierto y el agente sigue escribiendo en su log
- **THEN** la sesión y sus llamadas aparecen en la app sin reiniciar, y cada nueva llamada se añade en la siguiente relectura

#### Scenario: Sin duplicados al releer desde el principio
- **WHEN** un archivo de Codex ya leído en parte se relee desde el principio
- **THEN** cada llamada sigue contando una sola vez

#### Scenario: Base SQLite de un agente
- **WHEN** OpenCode confirma datos nuevos y solo cambia el archivo `-wal` de su base
- **THEN** la base del agente se relee y sus llamadas nuevas aparecen

### Requirement: Aviso de datos cambiados
Tras confirmar datos nuevos (relectura, escaneo, cambio de precios, limpieza o reconstrucción), el sistema SHALL avisar a la interfaz con un único evento `data:changed` que lleva solo la generación de datos, los agentes y los días locales afectados, nunca filas de datos, y MUST emitir como mucho uno por segundo: el primero tras un segundo de calma sale al momento y los siguientes se agrupan en uno al cumplirse el segundo.

#### Scenario: Ráfaga de cambios
- **WHEN** se escriben 500 eventos en 50 logs a lo largo de 3 s
- **THEN** la interfaz recibe como mucho un `data:changed` por segundo, el último con la generación final y la unión de agentes y días afectados

#### Scenario: Aviso sin datos
- **WHEN** la interfaz recibe `data:changed`
- **THEN** el evento contiene solo `generation`, `agents` y `days`, y la interfaz pide los datos que muestra

#### Scenario: Relectura sin cambios
- **WHEN** un evento del sistema de archivos no añade ni cambia ningún dato
- **THEN** no se emite `data:changed`

### Requirement: Sin trabajo en reposo
Sin eventos del sistema de archivos ni peticiones de la interfaz, ningún hilo del backend SHALL despertarse para revisar carpetas ni datos; el consumo de CPU del backend en reposo MUST ser despreciable. Si la cola de eventos del sistema operativo se desborda, el sistema SHALL hacer un escaneo completo para no perder cambios.

#### Scenario: App en reposo
- **WHEN** la app está abierta 60 s sin actividad de los agentes
- **THEN** el backend consume menos de 60 ms de CPU en ese tiempo y no hace ninguna relectura

#### Scenario: Cola de eventos desbordada
- **WHEN** el sistema operativo avisa de que se perdieron eventos
- **THEN** se hace un escaneo completo y los datos quedan como tras releer todo
