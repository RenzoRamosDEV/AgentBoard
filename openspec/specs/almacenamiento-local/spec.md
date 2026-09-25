# almacenamiento-local Specification

## Purpose
Define dónde guarda AgentBoard su base de datos local, cómo evoluciona el esquema y cómo se representan las fechas para que el historial sea fiable y portable.

## Requirements

### Requirement: Lectura concurrente
La base MUST permitir que la interfaz lea mientras la ingesta escribe (modo WAL).

#### Scenario: Consulta durante una importación
- **WHEN** la UI pide un resumen mientras se importa un lote
- **THEN** la consulta responde sin error con los datos confirmados hasta ese momento

### Requirement: Fechas en UTC
Las marcas de tiempo SHALL guardarse como milisegundos epoch UTC y convertirse a la zona local solo al mostrarse.

#### Scenario: Registro con zona horaria
- **WHEN** se importa una línea con `timestamp` `2026-09-25T09:57:51.467Z`
- **THEN** se guarda como `1790330271467`

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
