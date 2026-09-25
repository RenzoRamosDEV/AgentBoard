# Spec Delta

## Purpose

Define dónde guarda AgentBurn su base de datos local, cómo evoluciona el esquema y cómo se representan las fechas para que el historial sea fiable y portable.

## ADDED Requirements

### Requirement: Base de datos en la carpeta de datos del usuario
El sistema SHALL guardar su base SQLite en la carpeta de datos estándar del sistema operativo (`~/.local/share/agentburn/` en Linux, `%APPDATA%\agentburn\` en Windows, `~/Library/Application Support/agentburn/` en macOS), creándola si no existe.

#### Scenario: Primer arranque
- **WHEN** la app arranca y la carpeta de datos no existe
- **THEN** se crea la carpeta y un archivo `agentburn.db` dentro

### Requirement: Migraciones versionadas
El sistema SHALL aplicar al arrancar, en orden y una sola vez, las migraciones de esquema pendientes.

#### Scenario: Arranque con base ya migrada
- **WHEN** la app arranca con todas las migraciones aplicadas
- **THEN** no se ejecuta ninguna migración y los datos existentes se conservan

#### Scenario: Arranque tras actualizar la app
- **WHEN** la app incluye una migración nueva
- **THEN** solo esa migración se aplica y se registra su versión

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
