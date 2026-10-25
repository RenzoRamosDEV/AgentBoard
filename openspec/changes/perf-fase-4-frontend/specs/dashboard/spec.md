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
