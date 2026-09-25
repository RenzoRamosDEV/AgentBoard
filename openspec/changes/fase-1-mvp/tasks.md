# Tasks

## 1. Backend

- [ ] 1.1 Migración `0002` con tabla `settings` y `cache_savings_usd` en `call_costs`; verificar con test que la base de fase 0 migra sin perder datos
- [ ] 1.2 Ampliar `summary` con cache hit, ahorro y burn rate; verificar el ejemplo de 80 % y burn rate 0 con tests
- [ ] 1.3 Implementar `timeseries` por día u hora con desfase horario; verificar con test que dos llamadas del mismo día local caen en el mismo cubo
- [ ] 1.4 Implementar `breakdown` para project, branch, model, tool, command y activity; verificar con tests el agrupado de comandos (`git` = 2)
- [ ] 1.5 Implementar `list_agents`, `list_projects` (con coste del periodo) y `data_info`; verificar con tests
- [ ] 1.6 Implementar `settings` (get/set con validación de presupuesto) y exponer todos los comandos en Tauri; verificar con `cargo test` y `cargo check`

## 2. Interfaz

- [ ] 2.1 Tipos y llamadas en `src/lib/api.ts` y cálculo de periodos en `src/lib/period.ts`; verificar con `npm run build`
- [ ] 2.2 Barra lateral con agentes, proyectos, periodo, rango personalizado e info de datos; verificar en `tauri dev`
- [ ] 2.3 Paneles de KPIs, gasto del mes, proyecto/rama, modelos, herramientas y comandos con gráficas SVG; verificar en `tauri dev` con datos reales
- [ ] 2.4 Diálogo de ajustes con presupuesto mensual; verificar que persiste tras reiniciar
- [ ] 2.5 Actualizar README con las vistas disponibles
