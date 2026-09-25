# Spec Delta

## REMOVED Requirements

### Requirement: Base de datos en la carpeta de datos del usuario
**Reason**: el usuario no quiere que la app guarde datos; basta con leer los logs del ordenador.
**Migration**: al arrancar, la app borra `agentboard.db` (y sus `-wal`/`-shm`) de la carpeta de datos si existen.

## ADDED Requirements

### Requirement: Datos en memoria
El sistema SHALL mantener los datos en una base SQLite en memoria que se rellena al arrancar leyendo los logs del ordenador y MUST NOT escribir esos datos en disco.

#### Scenario: Reinicio de la app
- **WHEN** se cierra y se vuelve a abrir la app
- **THEN** los datos se vuelven a leer de los logs y no queda ningún archivo de base de datos

#### Scenario: Base heredada
- **WHEN** existe `~/.local/share/agentboard/agentboard.db` de una versión anterior
- **THEN** se elimina al arrancar

### Requirement: Esquema en memoria
El sistema SHALL aplicar en orden todas las migraciones de esquema a la base en memoria en cada arranque, antes del primer escaneo.

#### Scenario: Arranque
- **WHEN** la app arranca
- **THEN** la base en memoria tiene la última versión del esquema antes de leer ningún log

## REMOVED Requirements

### Requirement: Migraciones versionadas
**Reason**: sin base en disco no hay esquema previo que migrar; se sustituye por "Esquema en memoria".
**Migration**: ninguna; las migraciones siguen siendo archivos SQL versionados que se aplican en cada arranque.
