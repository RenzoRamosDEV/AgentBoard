# almacenamiento-local Specification

## Purpose
Define dónde guarda AgentBoard su base de datos local, cómo evoluciona el esquema y cómo se representan las fechas para que el historial sea fiable y portable.

## Requirements

### Requirement: Lectura concurrente
La interfaz SHALL leer la base por conexiones distintas de la del escritor y cada consulta MUST ver solo lotes de escritura confirmados enteros. Con la caché en disco, las consultas MUST NOT esperar a que termine un lote ni la escritura MUST esperar a que terminen las consultas; en modo solo memoria la espera MUST quedar acotada a un lote o a una consulta.

#### Scenario: Consulta durante una importación
- **WHEN** la UI pide un resumen mientras se importa un lote
- **THEN** la consulta responde sin error con los datos confirmados hasta ese momento

#### Scenario: Relectura durante el Resumen
- **WHEN** el vigilante pide una relectura mientras la interfaz ejecuta las consultas del Resumen con la caché en disco
- **THEN** el escritor confirma su lote sin esperar a que terminen las consultas

### Requirement: Fechas en UTC
Las marcas de tiempo SHALL guardarse como milisegundos epoch UTC y convertirse a la zona local solo al mostrarse.

#### Scenario: Registro con zona horaria
- **WHEN** se importa una línea con `timestamp` `2026-09-25T09:57:51.467Z`
- **THEN** se guarda como `1790330271467`

### Requirement: Caché local reconstruible
El sistema SHALL guardar los datos leídos de los logs, junto con el estado de lectura de cada archivo, en una caché SQLite en la carpeta de datos del usuario (`~/.local/share/agentboard/cache.sqlite` en Linux), de modo que el arranque solo lea lo nuevo. La caché MUST contener solo datos obtenidos de los logs, nunca el texto de prompts ni respuestas, MUST ser legible solo por el usuario y MUST poder borrarse sin perder nada que no vuelva a leerse de los logs.

#### Scenario: Reinicio de la app
- **WHEN** se cierra y se vuelve a abrir la app con la caché en disco
- **THEN** el Resumen muestra los datos guardados sin releer los logs que no han cambiado, y solo se leen los archivos nuevos o con líneas nuevas

#### Scenario: Sin texto de mensajes
- **WHEN** se importa una sesión con prompts y respuestas y se inspecciona la caché
- **THEN** ninguna tabla contiene el texto de esos mensajes

#### Scenario: Permisos del archivo
- **WHEN** se crea la caché en un sistema Unix
- **THEN** el archivo de la caché y sus archivos auxiliares solo tienen permisos de lectura y escritura para el usuario

#### Scenario: Base heredada
- **WHEN** existe `~/.local/share/agentboard/agentboard.db` o `agentburn.db` (con sus `-wal`/`-shm`) de una versión anterior
- **THEN** se eliminan al arrancar y la caché `cache.sqlite` de la misma carpeta se conserva

### Requirement: Esquema de la base
El sistema SHALL aplicar en orden todas las migraciones de esquema antes del primer escaneo. Una caché en disco cuya versión de esquema no es la que espera la app MUST borrarse y reconstruirse leyendo los logs, en lugar de migrarse.

#### Scenario: Arranque
- **WHEN** la app arranca sin caché o en modo solo memoria
- **THEN** la base tiene la última versión del esquema antes de leer ningún log

#### Scenario: Caché de otra versión
- **WHEN** la app arranca con una caché creada por una versión con otro esquema
- **THEN** la caché se borra, se crea vacía con el esquema actual y se rellena leyendo todos los logs, sin cerrar la app

### Requirement: Recuperación de una caché dañada
Si la caché no se puede abrir, no es una base SQLite válida o SQLite informa de corrupción al leerla o escribirla, el sistema SHALL borrar sus archivos y reconstruirla leyendo los logs, avisando por stderr, y MUST NOT cerrar la app. Si la carpeta de datos no permite escribir, la app MUST seguir funcionando en memoria durante esa sesión.

#### Scenario: Archivo corrupto
- **WHEN** la app arranca y `cache.sqlite` contiene bytes que no son una base SQLite
- **THEN** la app arranca, borra el archivo, lo reconstruye desde los logs y stderr muestra un aviso

#### Scenario: Carpeta sin permiso de escritura
- **WHEN** la carpeta de datos no permite crear la caché
- **THEN** la app funciona con los datos en memoria y stderr muestra un aviso

### Requirement: Caché fiel a los logs
Los datos SHALL reflejar solo los logs presentes: cuando un archivo de log ya leído desaparece, se sustituye por otro con la misma ruta o encoge, sus datos MUST retirarse y las sesiones a las que aportaba MUST rehacerse con sus otros archivos, de modo que el resultado sea el mismo que leer todos los logs desde cero.

#### Scenario: Log borrado por el agente
- **WHEN** un agente borra el log de una sesión antigua y la app vuelve a escanear (al arrancar o en vivo)
- **THEN** esa sesión deja de aparecer y las cifras coinciden con las de una reconstrucción completa de la caché

#### Scenario: Sesión repartida en varios archivos
- **WHEN** desaparece uno de los archivos que aportaban datos a una sesión
- **THEN** la sesión conserva exactamente los datos de sus archivos restantes, incluidas sus fechas de inicio y fin

### Requirement: Días locales coherentes con la zona horaria
Las agrupaciones por día local SHALL calcularse con la zona horaria del sistema y su horario de verano en cada fecha. Si la zona horaria del sistema cambia entre arranques, los días guardados en la caché MUST recalcularse antes de usarse para responder consultas.

#### Scenario: Cambio de zona horaria
- **WHEN** la caché se creó en `Europe/Madrid` y la app arranca con la zona del sistema `America/New_York`
- **THEN** la serie diaria del Resumen agrupa las llamadas por los días locales de `America/New_York`, igual que tras una reconstrucción completa
