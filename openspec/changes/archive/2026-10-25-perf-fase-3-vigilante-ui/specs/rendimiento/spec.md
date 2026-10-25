# Spec Delta

## MODIFIED Requirements

### Requirement: Ingesta fuera de la interfaz
Toda la ingesta (escaneo inicial y relecturas en vivo) SHALL ejecutarse en un único hilo de ingesta dedicado, fuera del hilo principal y fuera de los comandos invocados por la interfaz. Los comandos MUST NOT ejecutar ingesta ni I/O de disco síncrono en el hilo principal, y la base MUST quedar bloqueada para la ingesta solo mientras se escribe cada lote, no mientras se leen o parsean archivos.

#### Scenario: Comando durante el escaneo inicial
- **WHEN** la interfaz invoca un comando de consulta mientras el escaneo inicial está leyendo y parseando archivos
- **THEN** el comando responde con los datos confirmados hasta ese momento sin esperar a que termine el escaneo

#### Scenario: Relectura en vivo
- **WHEN** el vigilante detecta cambios en los logs
- **THEN** la relectura de los archivos cambiados la ejecuta el hilo de ingesta y la interfaz recibe el aviso `data:changed`

#### Scenario: Fin del escaneo inicial
- **WHEN** termina el escaneo inicial o el de una reconstrucción de la caché
- **THEN** la interfaz recibe además el aviso de ingesta terminada con sus estadísticas

## ADDED Requirements

### Requirement: Lecturas agrupadas y WAL acotado
La interfaz SHALL pedir los datos de cada vista en una sola petición por aviso de datos cambiados. Con la caché en disco, el escritor MUST reiniciar el WAL cuando quede libre y el WAL supere su límite, para que las lecturas no se degraden con el número de relecturas.

#### Scenario: Relecturas con la interfaz abierta
- **WHEN** se hacen 200 relecturas de 1000 líneas sobre 200 MB en disco mientras un lector pide el Resumen tras cada aviso, como la interfaz
- **THEN** la mediana de la relectura no supera 1,1 veces la medida sin lector

#### Scenario: WAL tras muchas relecturas
- **WHEN** termina una tanda de 200 relecturas sobre la caché en disco de 200 MB
- **THEN** el WAL no supera su límite de tamaño y el Resumen en frío tarda como mucho 1,2 veces lo que tardaba antes de la tanda

### Requirement: Respuestas paginadas y compactas
Los comandos que devuelven listas que crecen con el historial (sesiones, turnos, sesiones de un proyecto) SHALL devolver páginas acotadas con el total de elementos, y las series largas MUST enviarse como columnas numéricas. Solo exportar MAY devolver todos los datos del filtro.

#### Scenario: Página de sesiones
- **WHEN** la interfaz pide sesiones con `limit` 1000
- **THEN** recibe como mucho 500 sesiones y el total de sesiones del filtro

#### Scenario: Tamaño de respuesta
- **WHEN** se ejecutan los comandos de la interfaz sobre el fixture sintético de 200 MB con el filtro de 30 días, incluido el detalle de su sesión más grande
- **THEN** ninguna respuesta, salvo exportar, ocupa más de 1 MiB serializada

### Requirement: Presupuestos del vigilante y del Resumen
Medido con la herramienta de medición en compilación de distribución sobre el fixture sintético de 200 MB, la mediana de la latencia entre una escritura aislada en un log y el Resumen que la incluye SHALL ser menor de 500 ms; una ráfaga de 500 eventos MUST producir como mucho un aviso por segundo; y el Resumen completo en una sola petición MUST tardar menos de 300 ms en frío.

#### Scenario: Escritura aislada
- **WHEN** se mide 20 veces una línea nueva escrita en un log con al menos 1,5 s entre escrituras
- **THEN** la mediana entre la escritura y un Resumen que incluye la llamada es menor de 500 ms

#### Scenario: Ráfaga de 500 eventos
- **WHEN** se escriben 500 líneas en 50 logs en 3 s
- **THEN** ningún intervalo de un segundo contiene más de un aviso y no se hace ningún escaneo completo

#### Scenario: Resumen en una petición
- **WHEN** se pide el Resumen completo de 200 MB, con comparación, en una sola petición tras el escaneo
- **THEN** tarda menos de 300 ms y sus valores coinciden con los de las consultas por separado
