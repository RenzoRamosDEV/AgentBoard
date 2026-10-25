# Spec Delta

## MODIFIED Requirements

### Requirement: Detalle de proyecto
Al elegir un proyecto, el sistema SHALL mostrar sus cifras (coste, sesiones, turnos, tiempo activo, cache hit y compactaciones), el coste acumulado por día del periodo, el coste por actividad, los modelos y las ramas usadas, sus sesiones y la latencia p50/p95 de sus herramientas. Las sesiones del proyecto SHALL cargarse por páginas, con su total y con orden por columna en el servidor.

#### Scenario: Ir a una sesión
- **WHEN** el usuario pulsa una sesión en el detalle del proyecto
- **THEN** se abre el detalle de esa sesión, con enlace para volver al proyecto

#### Scenario: Proyecto con muchas sesiones
- **WHEN** un proyecto tiene 3000 sesiones en el periodo
- **THEN** el detalle muestra sus cifras y el número de sesiones con compactaciones calculados sobre todas ellas, pero recibe solo la primera página de sesiones

#### Scenario: Ordenar las sesiones del proyecto
- **WHEN** el usuario ordena las sesiones del proyecto por coste
- **THEN** la tabla muestra las sesiones más caras de todo el proyecto, no solo de las ya cargadas
