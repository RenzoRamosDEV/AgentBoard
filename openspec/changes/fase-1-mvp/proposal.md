# Proposal

## Why

La fase 0 importa el historial pero solo enseña un total. Para responder "¿cuánto llevo, a qué ritmo, en qué proyecto y con qué modelo?" hace falta el dashboard con sus filtros, que es lo que convierte la base en una herramienta útil a diario.

## What Changes

- Barra lateral con filtros: agentes y proyectos auto-detectados (buscador y casillas), periodo (Hoy, 7 días, 30 días, Mes, 6 meses, Todo, rango personalizado) e información de datos (primer registro, tamaño de la base, archivos vigilados).
- Paneles: KPIs (coste, llamadas, sesiones, cache hit, ahorro por caché, burn rate), gasto del mes (acumulado, proyección y presupuesto), coste por proyecto y por rama, modelos (coste y cache hit), herramientas y comandos de shell (uso y errores).
- Ajuste de presupuesto mensual, persistido en la base.
- Comandos nuevos: `get_timeseries`, `get_breakdown`, `list_agents`, `list_projects`, `get_data_info`, `get_settings`, `set_settings`; `get_summary` añade cache hit, ahorro y burn rate.

## Capabilities

### New Capabilities
- `filtros`: selección de agentes, proyectos y periodo que se aplica a todas las vistas.
- `dashboard`: paneles de coste y uso con las cifras que muestra cada uno.
- `ajustes`: preferencias del usuario guardadas en local (presupuesto mensual).

### Modified Capabilities

## Impact

- `src-tauri/src/queries.rs`, `commands.rs`; migración `0002` (tabla `settings` y ahorro por caché en `call_costs`).
- UI: `src/components/`, `src/views/Dashboard.tsx`, gráficas SVG propias sin dependencias nuevas.
