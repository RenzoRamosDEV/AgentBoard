## MODIFIED Requirements

### Requirement: Hoy en directo
La portada SHALL mostrar junto al gasto diario una tarjeta «Hoy en directo», independiente del periodo elegido pero con los filtros de agentes y proyectos, con: el gasto de hoy desde las 00:00 locales y su variación frente a ayer hasta la misma hora; el coste de los últimos 60 minutos (USD/h); el gasto por hora de hoy; y la sesión activa o, si no hay, la última de hoy, con enlace a su detalle. La tarjeta SHALL actualizarse con cada aviso de datos cambiados.

#### Scenario: Comparación justa con ayer
- **WHEN** a las 14:00 el gasto de hoy es 40 USD y ayer hasta las 14:00 fue 30 USD
- **THEN** la tarjeta muestra «▲ 33%» frente a ayer

#### Scenario: Sesión activa
- **WHEN** una sesión de Claude Code tuvo una llamada hace 3 minutos
- **THEN** la tarjeta la muestra como activa ahora, con enlace a su detalle

#### Scenario: Ventana de actividad de 15 minutos
- **WHEN** ninguna sesión tiene llamadas en los últimos 15 minutos y la última sesión de hoy tuvo su última llamada hace 16 minutos
- **THEN** la tarjeta no muestra ninguna sesión como activa y muestra esa sesión como la última de hoy, con enlace a su detalle

#### Scenario: Sin actividad hoy
- **WHEN** no hay llamadas hoy
- **THEN** la tarjeta muestra 0 USD y que aún no hay sesiones hoy
