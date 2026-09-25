# Proposal

## Why

La fase 0 importa el historial pero solo enseña un total. Para responder "¿cuánto llevo, a qué ritmo, en qué proyecto y con qué modelo?" hace falta el dashboard con sus filtros, que es lo que convierte la base en una herramienta útil a diario.

## What Changes

- Barra lateral con filtros: agentes y proyectos auto-detectados (buscador y casillas), periodo (Hoy, 7 días, 30 días, Mes, 6 meses, Todo, rango personalizado) e información de datos (primer registro, tamaño de la base, archivos vigilados).
- Paneles con la misma organización que CodeBurn: cabecera de resumen (coste, llamadas, sesiones, cache hit, tokens de entrada/salida/caché, burn rate, gasto del mes y presupuesto), Daily Activity, By Project (o por rama con un proyecto elegido), By Activity (con turnos y 1-shot), By Model (con cache hit y 1-shot), Core Tools, Shell Commands, Skills & Agents, MCP Servers y Claude Agent Types.
- Estética de terminal oscura: paneles con borde de color, barras de calor en degradado y cifras de coste resaltadas.
- La ingesta registra turnos (por `promptId`), la intención del prompt por palabras clave (sin guardar el texto), el detalle de cada herramienta (skill, tipo de subagente) y el `agentId` de las llamadas de subagentes. La migración fuerza una relectura para rellenar estos datos en el historial ya importado.
- Ajuste de presupuesto mensual, persistido en la base.
- Comandos nuevos: `get_timeseries`, `get_breakdown`, `get_activity`, `list_agents`, `list_projects`, `get_data_info`, `get_settings`, `set_settings`; `get_summary` añade cache hit, ahorro y burn rate.

## Capabilities

### New Capabilities
- `filtros`: selección de agentes, proyectos y periodo que se aplica a todas las vistas.
- `dashboard`: paneles de coste y uso con las cifras que muestra cada uno.
- `ajustes`: preferencias del usuario guardadas en local (presupuesto mensual).

### Modified Capabilities

## Impact

- `src-tauri/src/queries.rs`, `commands.rs`, `providers/claude_code.rs`, `ingest.rs`; migraciones `0002` (tabla `settings`, ahorro por caché) y `0003` (turnos, detalle de herramientas, subagentes).
- UI: `src/components/`, `src/views/Dashboard.tsx`, gráficas SVG propias sin dependencias nuevas.
