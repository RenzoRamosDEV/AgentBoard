# Spec Delta

## ADDED Requirements

### Requirement: Caché de datos de la interfaz
La interfaz SHALL guardar en memoria la última respuesta de cada consulta por comando, filtro y generación de datos, y reutilizarla mientras no llegue un aviso de datos cambiados ni cambien los ajustes. MUST NOT volver a pedir datos al recuperar el foco la ventana y MUST unir en una sola petición las peticiones iguales simultáneas.

#### Scenario: Dos vistas con la misma consulta
- **WHEN** dos componentes montados piden a la vez el mismo comando con el mismo filtro
- **THEN** se hace una sola petición al backend y ambos reciben la misma respuesta

#### Scenario: Ajustes guardados
- **WHEN** se guardan los ajustes (precios o presupuestos)
- **THEN** las consultas de las vistas montadas se vuelven a pedir y las demás quedan obsoletas

#### Scenario: Memoria acotada
- **WHEN** se recorren más filtros distintos que el límite de entradas de la caché
- **THEN** se descartan las entradas usadas hace más tiempo sin afectar a las de las vistas montadas

### Requirement: Tablas largas virtualizadas
Una tabla con más de 200 filas visibles SHALL pintar solo las filas dentro de la zona visible más un margen, conservando la altura total, el orden, el aspecto de cada fila y la navegación con teclado. Con 200 filas o menos MUST pintar todas.

#### Scenario: Tabla de 1000 filas
- **WHEN** se despliega una tabla con 1000 filas
- **THEN** el documento contiene como mucho las filas visibles más el margen, la barra de desplazamiento corresponde a las 1000 y al desplazarse aparecen las filas de esa posición

#### Scenario: Límite
- **WHEN** una tabla tiene 200 filas
- **THEN** se pintan las 200

### Requirement: Carga de código por apartado
El código de cada apartado distinto del Resumen (vista ampliada, Proyectos, detalles de proyecto y de sesión, Precios, Ajustes y buscador) SHALL cargarse al abrirlo por primera vez, mostrando mientras tanto el mismo indicador de carga que hoy.

#### Scenario: Arranque
- **WHEN** la app arranca en el Resumen
- **THEN** no se descarga el código de Proyectos ni de Precios hasta abrirlos

### Requirement: Aspecto sin cambios
Las mejoras de rendimiento de la interfaz SHALL NOT cambiar el diseño, los textos, las cifras ni el orden de lo que se muestra, y los gráficos MUST NOT animar sus cambios de geometría al refrescarse con datos nuevos.

#### Scenario: Refresco en vivo
- **WHEN** llega un aviso de datos cambiados con un gráfico visible
- **THEN** el gráfico se redibuja con los datos nuevos sin transición animada

#### Scenario: Mismas cifras
- **WHEN** se abre cada apartado con el mock de desarrollo antes y después del cambio
- **THEN** los textos y cifras mostrados son iguales

### Requirement: Presupuestos de interacción
Medido con la build de producción y los datos de ejemplo, con la latencia del Resumen en frío medida en 200 MB, la mediana entre un cambio de apartado o de filtro con datos ya cargados y el primer fotograma pintado con el resultado SHALL ser menor de 100 ms, y con datos sin cargar MUST ser menor de 300 ms.

#### Scenario: Cambio de apartado cacheado
- **WHEN** se cambia cinco veces entre el Resumen y un apartado ya visitado
- **THEN** la mediana hasta el fotograma pintado es menor de 100 ms

#### Scenario: Cambio de filtro en frío
- **WHEN** se cambia cinco veces a un periodo no cargado, con la latencia del Resumen de 200 MB
- **THEN** la mediana hasta el fotograma pintado con los datos nuevos es menor de 300 ms

### Requirement: Comparación de la interfaz documentada
La documentación de rendimiento SHALL incluir, antes y después de esta fase y con el mismo arnés, la latencia de cambio de apartado y de filtro (en frío y cacheada), el tiempo de render de React por escenario y el tamaño del bundle inicial y por apartado (sin comprimir y gzip). Cada paso que no mejore su métrica MUST revertirse y anotarse.

#### Scenario: Informe
- **WHEN** termina la fase
- **THEN** `docs/perf/BASELINE.md` tiene una sección «Después de la Fase 4» con esas cifras antes y después, los pasos conservados y revertidos y cómo reproducir la medición
