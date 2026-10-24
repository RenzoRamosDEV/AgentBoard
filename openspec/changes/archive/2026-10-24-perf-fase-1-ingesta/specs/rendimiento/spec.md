# Spec Delta

## ADDED Requirements

### Requirement: Ingesta fuera de la interfaz
Toda la ingesta (escaneo inicial y relecturas en vivo) SHALL ejecutarse en un único hilo de ingesta dedicado, fuera del hilo principal y fuera de los comandos invocados por la interfaz. Los comandos MUST NOT ejecutar ingesta ni I/O de disco síncrono en el hilo principal, y la base MUST quedar bloqueada para la ingesta solo mientras se escribe cada lote, no mientras se leen o parsean archivos.

#### Scenario: Comando durante el escaneo inicial
- **WHEN** la interfaz invoca un comando de consulta mientras el escaneo inicial está leyendo y parseando archivos
- **THEN** el comando responde con los datos confirmados hasta ese momento sin esperar a que termine el escaneo

#### Scenario: Relectura en vivo
- **WHEN** el vigilante detecta cambios en los logs
- **THEN** la relectura la ejecuta el hilo de ingesta y la interfaz recibe el aviso de ingesta terminada como hasta ahora

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
Por cada archivo de log el sistema SHALL recordar su identidad, offset, tamaño y fecha de modificación, leer solo desde el offset y saltar sin abrirlo un archivo que no ha cambiado. Si el archivo encoge o cambia su identidad (rotación), MUST releerse desde el principio sin duplicar datos; una última línea sin salto final MUST esperar a la siguiente pasada.

#### Scenario: Archivo rotado
- **WHEN** un archivo ya leído se sustituye por otro con la misma ruta y distinta identidad
- **THEN** se relee desde el byte 0 y las llamadas ya guardadas no se duplican

#### Scenario: Archivo sin cambios
- **WHEN** un escaneo encuentra un archivo con la misma identidad y su tamaño igual al offset guardado
- **THEN** el archivo no se abre ni se parsea

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
El proyecto SHALL documentar, junto a la línea base, las cifras de escaneo frío, refresco en vivo, memoria pico y benchmarks después de esta fase, con los mismos comandos de reproducción, e indicar qué pasos se revirtieron por no mejorar y por qué.

#### Scenario: Revisión de la fase
- **WHEN** un desarrollador consulta la documentación de rendimiento tras esta fase
- **THEN** encuentra las cifras antes y después para cada métrica de la línea base y la lista de pasos revertidos con su motivo
