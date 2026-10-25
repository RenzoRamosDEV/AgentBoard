# Spec Delta

## MODIFIED Requirements

### Requirement: Actualización tras la importación
Los apartados SHALL refrescarse cuando termine una importación y con cada aviso de datos cambiados, pidiendo los datos de nuevo como mucho una vez por aviso.

#### Scenario: Escaneo inicial termina
- **WHEN** el escaneo inicial acaba mientras el dashboard está abierto
- **THEN** las cifras se actualizan sin recargar la app

#### Scenario: Relectura en vivo
- **WHEN** llega un aviso de datos cambiados con el Resumen abierto
- **THEN** el Resumen se vuelve a cargar con una sola petición y muestra los datos nuevos

## ADDED Requirements

### Requirement: Resumen en una sola petición
El sistema SHALL ofrecer en una sola petición todo lo que muestra el Resumen para un filtro (cabecera, gasto del mes, series, desgloses, actividad y, con la comparación activa, el periodo anterior), calculado sobre un mismo estado confirmado de los datos y con los mismos valores que las consultas por separado.

#### Scenario: Valores iguales
- **WHEN** se pide el Resumen en una sola petición para un filtro y zona horaria
- **THEN** cada apartado coincide con la consulta individual equivalente (enteros exactos, importes con diferencia de como mucho 1e-6)

#### Scenario: Estado consistente
- **WHEN** se confirma un lote mientras se calcula el Resumen
- **THEN** todos los apartados devueltos corresponden al mismo estado de los datos

#### Scenario: Repetición sin datos nuevos
- **WHEN** se pide dos veces el mismo Resumen sin datos nuevos dentro del mismo minuto
- **THEN** la segunda respuesta sale de la caché de resultados
