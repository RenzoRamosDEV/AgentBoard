# Spec Delta

## MODIFIED Requirements

### Requirement: Lectura incremental por identidad de archivo
Por cada archivo de log el sistema SHALL recordar su identidad, offset, tamaño y fecha de modificación, leer solo desde el offset y saltar sin abrirlo un archivo que no ha cambiado. Si el archivo encoge o cambia su identidad (rotación), MUST releerse desde el principio sin duplicar datos; una última línea sin salto final MUST esperar a la siguiente pasada. Con la caché en disco, este estado MUST conservarse entre arranques.

#### Scenario: Archivo rotado
- **WHEN** un archivo ya leído se sustituye por otro con la misma ruta y distinta identidad
- **THEN** se relee desde el byte 0 y las llamadas ya guardadas no se duplican

#### Scenario: Archivo sin cambios
- **WHEN** un escaneo encuentra un archivo con la misma identidad y su tamaño igual al offset guardado
- **THEN** el archivo no se abre ni se parsea

#### Scenario: Arranque con caché
- **WHEN** la app arranca con una caché en disco y un log ha crecido desde el último cierre
- **THEN** solo se leen las líneas añadidas desde el offset guardado y el resto de archivos no se abre

### Requirement: Comparación documentada con la línea base
El proyecto SHALL documentar, junto a la línea base, las cifras de escaneo frío, refresco en vivo, memoria pico, benchmarks, arranque con caché y consultas del Resumen (en frío y en caché) después de cada fase, con los mismos comandos de reproducción, e indicar qué pasos se revirtieron por no mejorar y por qué.

#### Scenario: Revisión de la fase
- **WHEN** un desarrollador consulta la documentación de rendimiento tras esta fase
- **THEN** encuentra las cifras antes y después para cada métrica de la línea base y la lista de pasos revertidos con su motivo

#### Scenario: Revisión de las consultas
- **WHEN** un desarrollador consulta la documentación tras la fase de consultas
- **THEN** encuentra el tiempo de cada consulta del Resumen y del Resumen completo en 200 MB y 1 GB, en frío y en caché, y el del arranque con caché, frente a las cifras anteriores

## ADDED Requirements

### Requirement: Escritor único por lotes
Todas las escrituras en la base (ingesta, cambios de precios, retirada de datos de logs desaparecidos y reconstrucción) SHALL hacerse desde un único escritor, en transacciones por lote que se confirman como mucho cada 5 000 filas o cada 200 ms, lo que llegue antes. Los comandos de la interfaz MUST NOT escribir en la base por su cuenta.

#### Scenario: Escaneo largo
- **WHEN** se importa un fixture de 200 MB
- **THEN** los datos se confirman en lotes de como mucho 5 000 filas o 200 ms y la interfaz ve crecer los datos durante el escaneo

#### Scenario: Cambio de precios durante un escaneo
- **WHEN** el usuario guarda un precio mientras el escaneo inicial está en curso
- **THEN** el cambio se aplica entre dos lotes, sin error y sin perder ni duplicar datos

### Requirement: Coste precalculado por llamada
El coste, el ahorro por caché y si una llamada tiene precio SHALL guardarse al escribir la llamada, con la misma fórmula y el precio vigente en su fecha. Un cambio de precios, del usuario o por defecto en una versión nueva, MUST recalcular solo las llamadas y agregados de los modelos afectados, sin releer los logs, antes de que la siguiente consulta lo refleje.

#### Scenario: Precio editado
- **WHEN** el usuario cambia el precio de un modelo en Ajustes
- **THEN** la siguiente consulta del Resumen muestra el coste recalculado de ese modelo sin releer los logs y los demás modelos no se recalculan

#### Scenario: Precios sin cambios al arrancar
- **WHEN** la app arranca con una caché en disco y los precios efectivos son los mismos que al cerrarla
- **THEN** no se recalcula el coste de ninguna llamada

#### Scenario: Precio por defecto nuevo
- **WHEN** una versión nueva de la app cambia el precio por defecto de un modelo y arranca sobre una caché existente
- **THEN** el coste de las llamadas de ese modelo se recalcula con el precio nuevo

### Requirement: Agregados diarios incrementales
Resumen, Daily Activity, By Agent, By Project y By Model SHALL leer de agregados por día local que el escritor actualiza en la misma transacción de cada lote, recalculando solo los días y sesiones que el lote tocó. Una consulta cuyo periodo no son días locales completos, o con otra zona horaria, MUST responder desde las llamadas con el mismo resultado.

#### Scenario: Refresco en vivo
- **WHEN** el vigilante importa llamadas nuevas de hoy en una sesión
- **THEN** solo se recalculan los agregados de hoy de esa sesión y el Resumen siguiente incluye las llamadas nuevas

#### Scenario: Periodo que no es de días completos
- **WHEN** una consulta pide un periodo que empieza a mitad de un día local
- **THEN** el resultado es el mismo que calculando sobre todas las llamadas del periodo

### Requirement: Caché de resultados de consultas
Una consulta repetida con el mismo filtro y los mismos argumentos, sin datos nuevos confirmados, SHALL responderse desde una caché en memoria. Cada lote confirmado, cambio de precios o reconstrucción MUST invalidarla; un resultado que depende de la hora actual MUST NOT reutilizarse pasado su minuto; y la caché MUST guardar solo resultados de la generación de datos vigente, con un número máximo de entradas.

#### Scenario: Volver a un periodo
- **WHEN** la interfaz pide dos veces el Resumen del mismo periodo sin que lleguen datos nuevos
- **THEN** la segunda vez las consultas se responden desde la caché con los mismos resultados

#### Scenario: Datos nuevos
- **WHEN** se confirma un lote con llamadas nuevas y la interfaz repite el Resumen
- **THEN** las consultas se recalculan e incluyen las llamadas nuevas

### Requirement: Resultados de consultas idénticos
Las optimizaciones de la base SHALL NOT cambiar lo que devuelven las consultas: sobre los fixtures de test y sintéticos, para un conjunto fijo de filtros y zona horaria, las salidas de las consultas del Resumen y de los apartados MUST ser iguales a las obtenidas antes de esta fase (enteros exactos, importes en USD con diferencia de como mucho 1e-6), con la caché en disco y en modo solo memoria.

#### Scenario: Golden de consultas
- **WHEN** se ejecutan las consultas del Resumen sobre el fixture sintético de test con la versión nueva
- **THEN** sus salidas coinciden con el golden grabado antes de optimizar

#### Scenario: Huella de ingesta
- **WHEN** se escanean los fixtures con la versión nueva
- **THEN** la huella de ingesta coincide con los golden existentes sin volver a grabarlos

### Requirement: Presupuestos de arranque y consultas
Medido con la herramienta de medición en compilación de distribución, con 200 MB ya indexados en la caché el arranque hasta tener los datos del Resumen SHALL tardar menos de 1,5 s; el Resumen completo de 200 MB MUST tardar menos de 300 ms en frío y menos de 100 ms desde la caché de resultados; y cada consulta del Resumen sobre 1 GB MUST tardar menos de 300 ms.

#### Scenario: Arranque con caché de 200 MB
- **WHEN** se mide el arranque sobre una caché que ya contiene el fixture de 200 MB sin cambios
- **THEN** los datos del Resumen están listos en menos de 1,5 s

#### Scenario: Resumen de 200 MB
- **WHEN** se ejecutan las consultas del Resumen completo sobre 200 MB tras el escaneo y después se repiten
- **THEN** la primera tanda tarda menos de 300 ms y la repetida menos de 100 ms

#### Scenario: Consultas sobre 1 GB
- **WHEN** se ejecuta cada consulta del Resumen sobre el fixture de 1 GB
- **THEN** cada una tarda menos de 300 ms

#### Scenario: Escaneo frío con caché en disco
- **WHEN** se mide el escaneo frío del fixture de 200 MB escribiendo en la caché en disco
- **THEN** sigue tardando menos de 10 s
