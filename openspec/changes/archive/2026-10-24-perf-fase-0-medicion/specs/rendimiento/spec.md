# Spec Delta

## Purpose

Permite observar dónde gasta tiempo el backend de AgentBoard y medir su rendimiento de forma reproducible, sin cambiar lo que ve el usuario ni tocar sus logs reales.

## ADDED Requirements

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
