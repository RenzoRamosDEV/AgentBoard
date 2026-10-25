# Spec Delta

## ADDED Requirements

### Requirement: Dependencias optimizadas en desarrollo
La compilación de desarrollo SHALL compilar todas las dependencias con optimización máxima, sin cambiar la compilación de distribución. El código propio MAY compilarse con optimización baja en desarrollo solo si eso no alarga más de un 20 % la recompilación incremental tras tocar un archivo del backend.

#### Scenario: Compilación de desarrollo
- **WHEN** se compila el backend con el perfil de desarrollo
- **THEN** las dependencias se compilan con `opt-level = 3` y el perfil de distribución sigue igual

#### Scenario: Optimización del código propio
- **WHEN** activar `opt-level = 1` en el código propio alarga la recompilación incremental más de un 20 %
- **THEN** el código propio se queda sin esa optimización en desarrollo y la medición queda anotada

### Requirement: Benchmarks comparados en CI
La integración continua SHALL ejecutar en Linux, en cada Pull Request y en cada push a `main`, los benchmarks del backend sobre el fixture sintético de 50 MB generado en el propio job (sin versionarlo), y comparar la mediana de cada uno con la línea base versionada en `docs/perf/baseline.json`.

#### Scenario: Pull Request sin regresiones
- **WHEN** un Pull Request no empeora ningún benchmark más de un 20 % respecto a la línea base
- **THEN** el job de benchmarks termina en verde y muestra por benchmark la línea base, el valor actual y el cambio en porcentaje

#### Scenario: Fixture generado en el job
- **WHEN** se ejecuta el job de benchmarks
- **THEN** el fixture de 50 MB se genera en el job con el generador de datos sintéticos y no se lee de ningún archivo versionado

### Requirement: Fallo claro ante regresiones
El job de benchmarks MUST fallar si la mediana de algún benchmark supera en más de un 20 % la de la línea base, nombrando cada benchmark afectado con su valor de la línea base, su valor actual y el porcentaje de empeoramiento. Un empeoramiento de exactamente el 20 % MUST NOT hacer fallar el job.

#### Scenario: Regresión mayor del 20 %
- **WHEN** la mediana de un benchmark es 1,25 veces la de la línea base
- **THEN** el job falla y su salida nombra ese benchmark, ambos valores y «+25 %» frente al umbral del 20 %

#### Scenario: Justo en el umbral
- **WHEN** la mediana de un benchmark es exactamente 1,20 veces la de la línea base
- **THEN** ese benchmark no hace fallar el job

### Requirement: Línea base completa y medida en CI
La línea base SHALL medirse en el mismo tipo de runner que el job de benchmarks y registrar con qué commit, runner y CPU se midió. El job MUST fallar con un mensaje que indique cómo actualizarla si falta el archivo, si un benchmark de la línea base no aparece en la ejecución o si los resultados no se pueden leer; un benchmark nuevo sin línea base MUST NOT hacerlo fallar.

#### Scenario: Benchmark eliminado o renombrado
- **WHEN** la línea base contiene un benchmark que la ejecución no produce
- **THEN** el job falla nombrándolo e indica que hay que actualizar la línea base

#### Scenario: Benchmark nuevo
- **WHEN** la ejecución produce un benchmark que no está en la línea base
- **THEN** el job lo lista como «sin línea base» y no falla por él

#### Scenario: Actualizar la línea base
- **WHEN** un desarrollador necesita actualizar la línea base
- **THEN** cada ejecución del job publica una línea base candidata medida en el runner y la documentación explica cómo descargarla y versionarla

### Requirement: Resultados finales documentados
El proyecto SHALL documentar en `docs/perf/RESULTS.md`, para cada presupuesto de rendimiento del objetivo global, el valor antes de las optimizaciones, el valor después y si se cumple, con los comandos para reproducir cada medición, la lista de lo que no se hizo o no se cumplió con su motivo, y los próximos pasos recomendados.

#### Scenario: Revisión de los resultados
- **WHEN** un desarrollador consulta `docs/perf/RESULTS.md`
- **THEN** encuentra una fila por presupuesto (arranque, escaneo frío, vigilante, avisos, cambio de filtro o apartado en caché y en frío, CPU en reposo, memoria y perfil de distribución) con antes, después y estado, y de dónde sale cada cifra

#### Scenario: Lo que no se cumplió
- **WHEN** un presupuesto no se cumple o un paso no se aplicó
- **THEN** el informe lo lista con su cifra y su motivo, sin presentarlo como cumplido

### Requirement: Arquitectura de ingesta y guía de perfilado documentadas
La documentación de desarrollo SHALL incluir un diagrama de los hilos y canales que van de un cambio en un log a los datos que pinta la interfaz, y una guía para perfilar el backend y la interfaz con las herramientas del proyecto.

#### Scenario: Seguir un cambio en un log
- **WHEN** un desarrollador consulta la arquitectura
- **THEN** el diagrama muestra vigilante, debounce, relectura dirigida, hilo de ingesta y escritor, refresco de derivados, notificador con throttle, aviso `data:changed`, caché de consultas de la interfaz y lectura del Resumen por los lectores

#### Scenario: Perfilar
- **WHEN** un desarrollador quiere medir o perfilar un cambio
- **THEN** la guía da los comandos de trazas, fixtures, herramienta de medición, benchmarks, flamegraph con punteros de marco y arnés de la interfaz
