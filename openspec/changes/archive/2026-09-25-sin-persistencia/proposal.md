# Proposal

## Why

El usuario no quiere una base de datos guardada: AgentBoard debe limitarse a leer los logs que ya están en el ordenador y mostrarlos. Guardar una copia no aporta nada en su uso y ocupa disco.

## What Changes

- La base SQLite pasa a vivir en memoria: se rellena al arrancar leyendo los logs y desaparece al cerrar la app. **BREAKING**: ya no se conserva historial que el agente borre.
- Se elimina la base de versiones anteriores (`~/.local/share/agentboard/`) al arrancar.
- Los ajustes (presupuesto mensual) pasan a `~/.config/agentboard/settings.json`.
- El bloque "Datos" del panel muestra primer registro, archivos leídos, llamadas y hora del último escaneo.

## Capabilities

### New Capabilities

### Modified Capabilities
- `almacenamiento-local`: la base ya no se guarda en la carpeta de datos; las migraciones se aplican a la base en memoria en cada arranque.
- `ajustes`: el presupuesto persiste en un archivo JSON de configuración, no en la base.

## Impact

- `src-tauri/src/{db,settings,commands,lib,queries}.rs`, `examples/scan.rs`, `src/components/Sidebar.tsx`, README.
