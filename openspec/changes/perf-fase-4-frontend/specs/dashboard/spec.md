# Spec Delta

## MODIFIED Requirements

### Requirement: Actualización tras la importación
Los apartados visibles SHALL refrescarse cuando termine una importación y con cada aviso de datos cambiados, pidiendo los datos de nuevo como mucho una vez por aviso. Los apartados que no se están mostrando MUST NOT pedir datos al recibir el aviso; al volver a mostrarse MUST enseñar al momento los últimos datos cargados y pedir los nuevos una sola vez.

#### Scenario: Escaneo inicial termina
- **WHEN** el escaneo inicial acaba mientras el dashboard está abierto
- **THEN** las cifras se actualizan sin recargar la app

#### Scenario: Relectura en vivo
- **WHEN** llega un aviso de datos cambiados con el Resumen abierto
- **THEN** el Resumen se vuelve a cargar con una sola petición y muestra los datos nuevos

#### Scenario: Apartado no visible
- **WHEN** llega un aviso de datos cambiados con el Resumen abierto y el apartado Proyectos sin mostrar
- **THEN** no se pide ningún dato de Proyectos hasta que se abre, y al abrirlo se ven primero los datos anteriores y después los nuevos tras una sola petición

#### Scenario: Volver a un filtro ya cargado
- **WHEN** se cambia de filtro y se vuelve al anterior sin avisos de datos cambiados entre medias
- **THEN** el apartado muestra los datos del filtro anterior sin pedirlos de nuevo, también tras perder y recuperar el foco la ventana

### Requirement: Resumen en una sola petición
El sistema SHALL ofrecer en una sola petición todo lo que muestra el Resumen para un filtro (cabecera, gasto del mes, series, desgloses, actividad y, con la comparación activa, el periodo anterior), calculado sobre un mismo estado confirmado de los datos y con los mismos valores que las consultas por separado. La respuesta MUST indicar su cubo temporal (día, o mes natural local si el periodo abarca más de 400 días) y entregar las series diarias, por clave y de actividad, y la serie del periodo anterior, ya agrupadas en ese cubo.

#### Scenario: Valores iguales
- **WHEN** se pide el Resumen en una sola petición para un filtro y zona horaria cuyo periodo abarca 400 días o menos
- **THEN** el cubo es diario y cada apartado coincide con la consulta individual equivalente (enteros exactos, importes con diferencia de como mucho 1e-6)

#### Scenario: Periodo largo por meses
- **WHEN** se pide el Resumen de un periodo de más de 400 días
- **THEN** el cubo es mensual y cada serie tiene un punto por mes local y clave cuya suma coincide con la de sus días en la consulta individual (enteros exactos, importes con diferencia de como mucho 1e-6)

#### Scenario: Estado consistente
- **WHEN** se confirma un lote mientras se calcula el Resumen
- **THEN** todos los apartados devueltos corresponden al mismo estado de los datos

#### Scenario: Repetición sin datos nuevos
- **WHEN** se pide dos veces el mismo Resumen sin datos nuevos dentro del mismo minuto
- **THEN** la segunda respuesta sale de la caché de resultados
