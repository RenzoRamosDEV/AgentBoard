# rendimiento Specification

## Purpose
Permite observar dónde gasta tiempo el backend de AgentBoard y medir su rendimiento de forma reproducible, sin cambiar lo que ve el usuario ni tocar sus logs reales.

## Requirements

### Requirement: Trazas desactivadas por defecto
Sin la variable de entorno `AGENTBOARD_LOG`, el sistema SHALL operar con nivel de trazas `warn`: MUST NOT escribir spans ni mensajes de nivel `info`, `debug` o `trace`, y su coste en la ruta caliente MUST ser despreciable.

#### Scenario: Escaneo sin variable de entorno
- **WHEN** la app arranca sin `AGENTBOARD_LOG` y hace un escaneo
- **THEN** stderr no contiene spans ni mensajes de nivel `debug` o `info`

### Requirement: Avisos y errores visibles por defecto
Los errores y avisos que hoy se escriben en stderr (archivo ilegible, fallo del escaneo inicial, carpeta no vigilable, vigilante detenido, precios del usuario no aplicables, menú de bandeja no reconstruible) SHALL seguir escribiéndose en stderr sin configurar nada, tanto en la app como en el servidor MCP.

#### Scenario: Archivo ilegible sin variable de entorno
- **WHEN** un archivo de log no se puede leer durante el escaneo y `AGENTBOARD_LOG` no está definida
- **THEN** stderr muestra un aviso con la ruta del archivo y el motivo

#### Scenario: Servidor MCP
- **WHEN** el servidor MCP encuentra un archivo ilegible al escanear
- **THEN** el aviso sale por stderr y stdout solo contiene mensajes JSON-RPC

### Requirement: Trazas activables por entorno
Con `AGENTBOARD_LOG` definida, el sistema SHALL filtrar las trazas según su valor, con sintaxis de directivas por nivel y módulo (p. ej. `debug` o `agentboard_lib::ingest=trace`). Un valor no válido MUST degradar al nivel por defecto `warn` sin impedir el arranque.

#### Scenario: Valor no válido
- **WHEN** la app arranca con `AGENTBOARD_LOG` con un valor que no es una directiva válida
- **THEN** la app arranca con normalidad y se comporta como sin la variable

### Requirement: Spans de las operaciones clave
Con nivel `debug`, el sistema SHALL emitir al cerrarse un span con su duración para: el recorrido de carpetas, la ingesta de cada archivo (con bytes, líneas y registros), la transacción de inserción de cada archivo, cada comando invocado por la interfaz, cada consulta agregada que esos comandos ejecutan y cada emisión del aviso de ingesta terminada a la interfaz.

#### Scenario: Trazas activadas
- **WHEN** la app se ejecuta con `AGENTBOARD_LOG=debug` y se hace un escaneo y se invoca un comando
- **THEN** stderr muestra spans de escaneo, de archivo, de inserción y de comando, cada uno con su duración

#### Scenario: Métricas por archivo
- **WHEN** se ingiere un archivo con `AGENTBOARD_LOG=debug`
- **THEN** su span incluye los bytes leídos, las líneas procesadas y los registros guardados

### Requirement: Comportamiento funcional sin cambios
La instrumentación y los perfiles de compilación SHALL NOT cambiar los datos importados, las cifras de las consultas, los comandos expuestos a la interfaz ni las herramientas del servidor MCP.

#### Scenario: Mismo resultado con y sin trazas
- **WHEN** se escanea la misma carpeta de logs con `AGENTBOARD_LOG=debug` y sin la variable
- **THEN** el número de archivos, líneas y registros importados y el resumen calculado son idénticos

### Requirement: Datos sintéticos deterministas
El proyecto SHALL incluir un generador de logs sintéticos para Claude Code, Codex, Copilot, Cursor y Gemini, en el mismo formato que los fixtures de test, con tamaño objetivo y semilla como parámetros y sin texto real. Con la misma semilla y tamaño MUST producir archivos idénticos byte a byte, con tamaño total dentro de ±5 % del objetivo.

#### Scenario: Generador determinista
- **WHEN** se ejecuta el generador dos veces con la misma semilla y el mismo tamaño
- **THEN** los archivos generados son idénticos byte a byte y su tamaño total está dentro de ±5 % del objetivo

#### Scenario: Formato compatible
- **WHEN** AgentBoard escanea una carpeta generada
- **THEN** importa llamadas de los cinco agentes sin errores de lectura

### Requirement: Medición aislada de los logs reales
Las herramientas de medición SHALL leer únicamente las carpetas indicadas por argumento y MUST NOT leer ni modificar los logs reales del usuario. Cualquier archivo que creen para simular actividad en vivo MUST quedar dentro de la carpeta indicada y eliminarse al terminar.

#### Scenario: Escaneo frío sobre fixtures
- **WHEN** se ejecuta la medición sobre una carpeta de fixtures
- **THEN** solo se importan archivos bajo esa carpeta y se imprimen los tiempos de escaneo frío, refresco en vivo y consultas del Resumen y la memoria pico

### Requirement: Línea base documentada
El proyecto SHALL documentar una línea base de rendimiento con el entorno de medición, los comandos exactos para reproducirla, los tiempos de escaneo frío por tamaño, refresco en vivo y consultas del Resumen, la memoria pico, los resultados de los benchmarks, los flamegraphs y un ranking de los 5 cuellos de botella con su evidencia.

#### Scenario: Reproducibilidad
- **WHEN** un desarrollador sigue los comandos documentados en una máquina con el entorno de desarrollo
- **THEN** obtiene los mismos fixtures y las mismas mediciones (con la variación propia del hardware)

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

### Requirement: Escaneo paralelo determinista
El escaneo SHALL leer y parsear archivos en paralelo, con un número de hilos no mayor que el número de cores, y MUST aplicar los resultados a la base en el mismo orden que el escaneo secuencial (agente, carpeta raíz y ruta ordenada), de modo que el resultado no dependa del reparto entre hilos.

#### Scenario: Mismo resultado en ejecuciones repetidas
- **WHEN** se escanea dos veces la misma carpeta de logs en bases nuevas
- **THEN** las llamadas, herramientas, turnos, sesiones y eventos guardados son idénticos en ambas ejecuciones, incluida la asignación de turnos

#### Scenario: Agentes con estado entre líneas
- **WHEN** se escanean en paralelo archivos de agentes que solo identifican la sesión en sus primeras líneas
- **THEN** cada archivo se interpreta con su propio estado y sus llamadas se asignan a la sesión correcta

### Requirement: Escritura sin recompilar sentencias
El escritor de la base SHALL reutilizar las sentencias SQL preparadas durante cada transacción o lote en lugar de compilarlas de nuevo por cada registro.

#### Scenario: Escaneo de un archivo grande
- **WHEN** se importa un archivo con miles de registros
- **THEN** cada sentencia distinta se compila como mucho una vez por transacción

### Requirement: Relleno de turnos acotado a la pasada
El relleno del turno de llamadas y herramientas que llegaron antes que su turno SHALL ejecutarse solo cuando la pasada actual importa turnos nuevos, y solo sobre las filas de las sesiones de esos turnos que aún no tienen turno. El coste de un refresco en vivo sin turnos nuevos MUST NOT crecer con el tamaño de la sesión.

#### Scenario: Refresco sin turnos nuevos
- **WHEN** una sesión grande recibe líneas nuevas sin prompts de usuario y se relee
- **THEN** no se recorren las llamadas anteriores de la sesión y el tiempo de refresco se mantiene estable a lo largo de refrescos sucesivos

#### Scenario: Subagente leído antes que su sesión
- **WHEN** se importan las llamadas de un subagente antes que el archivo de la sesión que contiene su turno
- **THEN** al importar el turno, esas llamadas quedan asignadas a él igual que antes de esta fase

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

### Requirement: Parseo mínimo sin materializar texto
Al leer logs de agentes, el sistema SHALL descartar las líneas que no aportan sesiones, llamadas, herramientas, turnos ni eventos comprobando su tipo antes de interpretarlas enteras, y MUST NOT copiar a memoria propia el texto de los mensajes más allá de lo necesario para clasificarlos.

#### Scenario: Línea irrelevante
- **WHEN** un log contiene una línea de un tipo que no produce filas
- **THEN** la línea se descarta sin interpretar el resto de sus campos y el resultado importado no cambia

#### Scenario: Prompt largo
- **WHEN** se importa un prompt de usuario de varios megabytes
- **THEN** se obtiene su intención sin guardar ni copiar el texto del mensaje

### Requirement: Fallos aislados sin cierre de la app
Un archivo ilegible o corrupto SHALL contarse como error, avisarse con su ruta y no detener la importación del resto. Ninguna ruta alcanzable desde los hilos de ingesta, vigilante o refresco de avisos MUST provocar un pánico ante datos inesperados, porque en el perfil de distribución un pánico cierra la app.

#### Scenario: Archivo ilegible entre otros válidos
- **WHEN** un escaneo en paralelo encuentra un archivo que no se puede leer entre varios archivos correctos
- **THEN** los archivos correctos se importan, el escaneo cuenta un error con la ruta del archivo y la app sigue abierta

#### Scenario: Líneas corruptas
- **WHEN** un archivo contiene líneas que no son JSON válido o bytes no UTF-8 entre líneas correctas
- **THEN** esas líneas se ignoran y el resto del archivo se importa

#### Scenario: Marca de tiempo fuera de rango
- **WHEN** el cálculo de inicio de día u hora recibe una marca de tiempo que no se puede representar en la zona horaria
- **THEN** devuelve un valor sin provocar un pánico

### Requirement: Resultados idénticos a la línea base
Las optimizaciones de ingesta SHALL NOT cambiar los datos importados: sobre los fixtures sintéticos, el número de llamadas, herramientas, turnos, sesiones y eventos, los tokens y los costes MUST ser idénticos a los obtenidos antes de esta fase, y el servidor MCP y los fixtures de test MUST seguir funcionando.

#### Scenario: Huella antes y después
- **WHEN** se escanea el mismo fixture sintético con la versión anterior y con la nueva
- **THEN** la huella de resultados (conteos, tokens y coste por agente, y el contenido de las filas por clave natural) es idéntica

### Requirement: Presupuestos de ingesta
Medido con la herramienta de medición en compilación de distribución sobre el fixture sintético de 200 MB, el escaneo frío SHALL tardar menos de 10 s y la mediana de 200 refrescos en vivo de 1000 líneas MUST ser menor de 200 ms, sin crecer con el tamaño de la sesión.

#### Scenario: Escaneo frío de 200 MB
- **WHEN** se mide el escaneo frío del fixture de 200 MB
- **THEN** tarda menos de 10 s

#### Scenario: Refresco que no crece
- **WHEN** se miden 200 refrescos seguidos de 1000 líneas sobre el fixture de 200 MB
- **THEN** la mediana es menor de 200 ms y la mediana de los últimos 20 refrescos no supera 1,5 veces la de los primeros 20

### Requirement: Comparación documentada con la línea base
El proyecto SHALL documentar, junto a la línea base, las cifras de escaneo frío, refresco en vivo, memoria pico, benchmarks, arranque con caché y consultas del Resumen (en frío y en caché) después de cada fase, con los mismos comandos de reproducción, e indicar qué pasos se revirtieron por no mejorar y por qué.

#### Scenario: Revisión de la fase
- **WHEN** un desarrollador consulta la documentación de rendimiento tras esta fase
- **THEN** encuentra las cifras antes y después para cada métrica de la línea base y la lista de pasos revertidos con su motivo

#### Scenario: Revisión de las consultas
- **WHEN** un desarrollador consulta la documentación tras la fase de consultas
- **THEN** encuentra el tiempo de cada consulta del Resumen y del Resumen completo en 200 MB y 1 GB, en frío y en caché, y el del arranque con caché, frente a las cifras anteriores

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
