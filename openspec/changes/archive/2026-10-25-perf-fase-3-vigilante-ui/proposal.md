# Proposal

## Why

Tras la Fase 2 las consultas son rápidas, pero el camino «un agente escribe → la interfaz lo muestra» sigue siendo caro y ruidoso: el vigilante pide un escaneo completo de todas las raíces por cada ráfaga (debounce fijo de 400 ms), revisa cada 20 s si han aparecido carpetas nuevas aunque no pase nada, y cada relectura dispara en la interfaz 19–23 comandos del Resumen. Esas ráfagas de lecturas retrasan las relecturas del vigilante, impiden que el WAL se reinicie (contención en disco ×1,37 frente al objetivo ×1,1) y `activity_daily` sigue en 341 ms con 1 GB (objetivo < 300 ms). Esta fase cierra el ciclo vigilante → escritor → interfaz con latencia baja, refrescos acotados y cero trabajo en reposo.

## What Changes

- **Vigilante dirigido**: debounce propio por archivo de 200 ms (dentro de 150–300 ms, con espera máxima de 1 s para archivos que no paran de crecer); se ignoran eventos `Access` y rutas que ningún proveedor reconoce; solo se releen los archivos tocados (relectura dirigida en el hilo de ingesta, sin recorrer las raíces). El escaneo completo queda para el arranque, la reconstrucción y el desbordamiento de la cola de eventos del SO.
- **Sin sondeo en reposo**: las carpetas de logs que aún no existen se detectan con eventos del primer antepasado existente; desaparece el intervalo de 20 s. Ningún hilo del backend se despierta sin eventos.
- **Evento `data:changed { generation, agents, days }`** sin datos, con throttle de 1 por segundo (primer aviso inmediato, los siguientes agrupados), tras cada transacción confirmada. Sustituye a `ingest://done` como señal de refresco de datos; **BREAKING (interno)**: `ingest://done` ya no se emite en las relecturas del vigilante y queda solo como fin del escaneo inicial y de una reconstrucción. Bandeja y avisos de presupuesto se refrescan al ritmo de `data:changed`.
- **`get_dashboard_snapshot(request)`**: un comando devuelve todo lo que necesita el Resumen (incluido el periodo anterior de la comparación) en una sola lectura consistente y con caché de resultados; `useDashboardData` pasa a usarlo (cambio mínimo de interfaz).
- **Paginación y tipos compactos**: `list_sessions` pasa a páginas (`limit ≤ 500`, `offset`, orden por columna permitida) con total; el detalle de proyecto deja de incluir todas sus sesiones y el de sesión deja de incluir todos sus turnos (comandos de página nuevos); la línea de coste de una sesión se envía en columnas numéricas. **BREAKING (interno)**: cambian las firmas de `list_sessions`, `get_project_detail` y `get_session_detail`. Exportar sigue devolviendo todo, por ser explícito. El servidor MCP no cambia.
- **Pendientes de la Fase 2**: checkpoint `TRUNCATE` del WAL cuando el escritor queda libre y el WAL supera su límite; lecturas de la interfaz agrupadas (un snapshot por aviso); `activity_daily` agregada en SQL por turno y día local (y, si no basta, tabla derivada por turno).
- **Medición**: perfil `bench-fast` (release sin LTO) para los pasos intermedios; `perf_scan` añade `--phase watch` (latencia y refrescos por ráfaga), `--phase idle` (CPU en reposo), lector «como la interfaz» en `--phase contention`, el snapshot en `--phase queries` y el tamaño máximo de respuesta. Medición release completa solo al cerrar, con sección «Después de la Fase 3» en `docs/perf/BASELINE.md`.

## Capabilities

### New Capabilities
<!-- ninguna -->

### Modified Capabilities
- `vigilancia-en-vivo`: relectura dirigida por archivo con latencia < 500 ms, debounce por archivo y filtrado de eventos, detección de carpetas nuevas por eventos y sin sondeo, aviso `data:changed` con throttle.
- `rendimiento`: la relectura en vivo avisa con `data:changed`; presupuestos del vigilante (latencia, refrescos por ráfaga, CPU en reposo), de contención con lecturas agrupadas y WAL acotado, y del snapshot del Resumen; respuestas paginadas y compactas.
- `dashboard`: el Resumen se carga con una sola petición y se refresca con `data:changed`.
- `sesiones`: listado paginado con orden en el servidor; turnos del detalle por páginas; línea de coste compacta.
- `proyectos`: las sesiones del detalle de proyecto se cargan por páginas.
- `interfaz`: el indicador en vivo y «Hoy en directo» se actualizan con `data:changed`.

## Impact

- Backend: `watcher.rs` (reescrito), `ingest.rs` (planificación por rutas), `ingest_thread.rs` (`Job::Rescan`, checkpoint), `derived.rs` (claves tocadas), módulos nuevos `notifier.rs` (throttle) y `snapshot.rs`, `insights.rs` (`activity_daily`), `sessions.rs` (páginas), `commands.rs`, `lib.rs`, `db.rs` (checkpoint); `examples/perf_scan.rs`, `benches/common/mod.rs`, `Cargo.toml` (perfil `bench-fast`).
- Frontend: `src/lib/api.ts`, `src/lib/useData.ts`, `src/App.tsx`, `src/views/TrayPanel.tsx`, `TodayLive.tsx`, `ProjectDetail.tsx`, `SessionDetail.tsx`, `src/components/DataTable.tsx` (orden y «más» controlados), `CommandPalette.tsx`, `SettingsDialog.tsx`/`src/lib/cache.ts` (siguen con `ingest://done`), `src/dev/mock.ts`.
- Servidor MCP: sin cambios de herramientas ni de salida (usa las funciones de dominio completas).
- Dependencias: ninguna nueva (debounce y throttle propios sobre `notify` 6).
- Documentación: `docs/perf/BASELINE.md`, `docs/wiki/Arquitectura.md`.
