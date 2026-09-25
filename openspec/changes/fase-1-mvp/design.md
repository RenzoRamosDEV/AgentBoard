# Design

## Context

Fase 0 dejó `call_costs`, `queries::Filter` y un único comando. La UI es un `App.tsx` mínimo. No hay librería de gráficas.

## Goals / Non-Goals

**Goals:**
- Todas las cifras salen de SQL agregando en Rust; la UI solo pinta.
- Filtros en un único estado React que se pasa a cada comando.

**Non-Goals:**
- Feed en vivo, scatter de sesiones, heatmap y "¿y si…?" (fases 2 y 4).
- Multimoneda: todo en USD por ahora.

## Decisions

- **Periodos calculados en la UI** (hora local) y enviados como `from`/`to` epoch ms: el backend no sabe de zonas. Para agrupar por día, `get_timeseries` recibe `tzOffsetMin` y suma el desfase antes de dividir; los cambios de horario de verano desplazan como mucho una hora del día del cambio.
- **Migración 0002**: tabla `settings (key, value)` con JSON por valor, y `call_costs` recreada con `cache_savings_usd`. Alternativa descartada: calcular el ahorro en cada consulta con otro JOIN a `prices` (duplica la lógica de vigencia).
- **Burn rate** = coste con `ts >= now − 60 min` con los filtros de agente y proyecto, ignorando el periodo: responde "a qué ritmo voy ahora".
- **`get_breakdown(filter, by)`** con `by ∈ project | branch | model | tool | command | activity` y filas homogéneas `{key, label, costUsd, calls, errors, cacheHit}`; herramientas y comandos no tienen coste propio (van en 0).
- **Gráficas SVG propias** (barras horizontales, línea acumulada): pocas formas, cero dependencias, control total del tema claro/oscuro. Recharts queda como opción si las fases 4–5 lo piden.
- **Excluir en vez de incluir**: la UI guarda los agentes/proyectos *ocultos* y envía la lista de incluidos solo si hay alguno oculto; así un agente nuevo aparece incluido por defecto.

## Risks / Trade-offs

- [Consultas lentas con historiales grandes] → índices por `ts` y `session_id` ya existen; si hace falta, índice compuesto en fase 4.
- [Rama "HEAD" en carpetas sin git] → se muestra tal cual; se etiqueta "(sin rama)" cuando es nula.
