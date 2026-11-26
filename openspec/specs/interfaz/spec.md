# interfaz Specification

## Purpose
Organiza la ventana de AgentBoard: un panel lateral para navegar, una barra superior con los filtros y una portada que prioriza lo importante.

## Requirements

### Requirement: Panel lateral agrupado
El panel lateral SHALL mostrar el logo de AgentBoard (versión oscura en tema oscuro y clara en tema claro) y los apartados agrupados bajo los encabezados *Coste*, *Trabajo* y *Herramientas*, conservando dentro de cada grupo el orden de apartados de la v0.1 (By Activity pasa a *Trabajo*), con Ajustes al pie. SHALL poder colapsarse a solo iconos y recordar ese estado.

#### Scenario: Tema claro
- **WHEN** el tema activo es claro
- **THEN** el panel muestra la versión clara del logo

#### Scenario: Navegar
- **WHEN** el usuario pulsa «By Model» en el grupo *Coste*
- **THEN** se abre la vista ampliada de By Model y el elemento queda marcado como activo

### Requirement: Barra de filtros
Los filtros SHALL estar en una barra superior común a todas las vistas: periodo (7, 30, 60, 90 días y Todo) como control segmentado, y agentes y proyectos como desplegables con casillas. El desplegable de proyectos SHALL permitir buscar y elegir «solo este proyecto». Los botones SHALL resumir la selección (p. ej. «Agentes · 3 de 6»).

#### Scenario: Ocultar un agente
- **WHEN** el usuario desmarca Codex en el desplegable de agentes
- **THEN** todas las vistas se recalculan sin Codex y el botón muestra «5 de 6»

#### Scenario: Cerrar el desplegable
- **WHEN** el usuario pulsa fuera del desplegable o la tecla Escape
- **THEN** el desplegable se cierra

### Requirement: Indicador en vivo y exportar
La barra SHALL mostrar de forma permanente que los datos están en vivo y cuánto hace de la última relectura con datos nuevos, y SHALL ofrecer exportar las llamadas del filtro activo a CSV o JSON.

#### Scenario: Relectura
- **WHEN** llega un aviso de datos cambiados tras una relectura del vigilante
- **THEN** el indicador pasa a «hace 0 s» y se resalta brevemente

### Requirement: Portada con jerarquía
La portada SHALL mostrar, en este orden: los KPIs del resumen; el gasto diario apilado por agente junto a By Agent; y Top proyectos, Top modelos y el reparto del coste por actividad, cada uno con enlace a su vista ampliada.

#### Scenario: Ir al detalle
- **WHEN** el usuario pulsa «By Project ›» en la tarjeta Top proyectos
- **THEN** se abre la vista ampliada de By Project

### Requirement: Buscador rápido
La barra superior SHALL ofrecer un buscador que se abre con Ctrl+K (o Cmd+K) o pulsándolo, y que permite saltar a un apartado, filtrar por un solo proyecto o abrir una sesión reciente, eligiendo con el teclado o el ratón. Escape SHALL cerrarlo.

#### Scenario: Ir a un proyecto
- **WHEN** el usuario pulsa Ctrl+K, escribe «tuio» y elige el proyecto tuio-api
- **THEN** el filtro queda solo con tuio-api

### Requirement: Detalles del panel lateral
El panel lateral SHALL mostrar el distintivo «V2», marcar como «nuevo» los apartados añadidos en la V2, incluir Ajustes en el grupo *Configuración* y mostrar al pie cuántos agentes y sesiones se han leído.

#### Scenario: Pie
- **WHEN** hay 6 agentes y 214 sesiones cargadas
- **THEN** el pie muestra «6 agentes · 214 sesiones leídas»

### Requirement: Detalles de la portada
Con la comparación activa, la portada SHALL indicar las fechas del periodo anterior. El KPI del mes SHALL mostrar el porcentaje del presupuesto cuando lo hay. El gasto diario SHALL mostrar eje de importes y el día de mayor gasto. El aviso de modelos sin precio SHALL ir dentro de la tarjeta Top modelos.

#### Scenario: Presupuesto en el KPI
- **WHEN** el gasto del mes es 412,80 USD y el presupuesto 500 USD
- **THEN** el KPI se titula «Mes · 83 % de $500.00»

### Requirement: Consistencia de tamaños
Todos los controles interactivos (botones, botones de filtro, desplegables, campos de texto, buscadores y controles segmentados) SHALL compartir altura y tamaño de letra, salvo los campos dentro de tablas, que usan una variante compacta común. Todos ellos SHALL compartir además radio de esquina 0, como las tarjetas. Ningún control ni etiqueta SHALL partirse en dos líneas.

#### Scenario: Barra de herramientas
- **WHEN** una barra combina un buscador, un botón y un control segmentado
- **THEN** los tres tienen la misma altura

#### Scenario: Radio de los controles
- **WHEN** una barra combina un botón de filtro, un buscador, un botón y un control segmentado
- **THEN** todos tienen esquinas rectas

### Requirement: Tarjetas ordenadas
Las tarjetas de una misma fila SHALL tener la misma altura, y ninguna SHALL mostrar un hueco vacío por estirarse a la altura de otra: el contenido que crece (gráficos) ocupa el espacio y el que sobra (listas largas) se desplaza dentro de la tarjeta. A partir de 1280 px de ancho ninguna tabla SHALL salirse de su tarjeta.

#### Scenario: Portada
- **WHEN** la tarjeta de avisos tiene diez avisos
- **THEN** mantiene la altura de la tarjeta del gasto diario y sus avisos se desplazan dentro

### Requirement: KPIs alineados
En cada fila de KPIs, las etiquetas SHALL ocupar una sola línea y los valores SHALL quedar a la misma altura; en ventanas de menos de 1400 px los seis KPIs del resumen SHALL repartirse en dos filas de tres.

#### Scenario: Ventana estrecha
- **WHEN** la ventana mide 1280 px
- **THEN** los KPIs del resumen se ven en dos filas de tres con las etiquetas completas

### Requirement: Hoy en directo
La portada SHALL mostrar junto al gasto diario una tarjeta «Hoy en directo», independiente del periodo elegido pero con los filtros de agentes y proyectos, con: el gasto de hoy desde las 00:00 locales y su variación frente a ayer hasta la misma hora; el coste de los últimos 60 minutos (USD/h); el gasto por hora de hoy; y la sesión activa (con llamadas en los últimos 15 minutos) o, si no hay, la última sesión de hoy, con enlace a su detalle. La tarjeta SHALL actualizarse con cada aviso de datos cambiados.

#### Scenario: Comparación justa con ayer
- **WHEN** a las 14:00 el gasto de hoy es 40 USD y ayer hasta las 14:00 fue 30 USD
- **THEN** la tarjeta muestra «▲ 33%» frente a ayer

#### Scenario: Sesión activa
- **WHEN** una sesión de Claude Code tuvo una llamada hace 3 minutos
- **THEN** la tarjeta la muestra como activa ahora, con enlace a su detalle

#### Scenario: Sin actividad hoy
- **WHEN** no hay llamadas hoy
- **THEN** la tarjeta muestra 0 USD y que aún no hay sesiones hoy

### Requirement: Paleta Grafito
La interfaz SHALL usar una paleta neutra de grises y negros en ambos modos. Oscuro: fondo `#0f0f10`; superficies `#161618`, `#1c1c1f`, `#232326`; borde `#262629`; separador de filas `#1e1e21`; texto primario `#ededee` y secundario `#b4b4b8`; pista de barras `#232326`. Claro: fondo `#f4f4f5`; superficies `#ffffff`, `#f0f0f2`, `#ececef`; borde `#e4e4e7`; separador `#f0f0f2`; texto `#111113` y `#4a4a50`; pista `#ececef`.

#### Scenario: Modo oscuro
- **WHEN** el tema activo es oscuro
- **THEN** el fondo de la ventana es `#0f0f10` y el texto primario `#ededee`

#### Scenario: Modo claro
- **WHEN** el usuario elige el tema claro en el selector de tema
- **THEN** el fondo de la ventana es `#f4f4f5` y el texto primario `#111113`

#### Scenario: Tema del sistema
- **WHEN** el usuario elige «sistema» y el sistema pide modo claro
- **THEN** la interfaz muestra la paleta Grafito clara

### Requirement: Texto atenuado legible
El texto atenuado SHALL tener un contraste de al menos 4.5:1 sobre el fondo de la ventana en ambos modos, partiendo de `#77777d` (oscuro) y `#8a8a92` (claro) y oscureciéndolo o aclarándolo lo mínimo necesario cuando no alcance ese contraste.

#### Scenario: Contraste en oscuro
- **WHEN** se mide el contraste del texto atenuado sobre `#0f0f10`
- **THEN** es igual o superior a 4.5:1

#### Scenario: Contraste en claro
- **WHEN** se mide el contraste del texto atenuado sobre `#f4f4f5`
- **THEN** es igual o superior a 4.5:1

### Requirement: Tarjetas rectas con borde degradado
Las tarjetas (paneles, KPIs y cualquier contenedor tipo tarjeta) SHALL tener esquinas rectas, sin sombra ni relleno propio, y un borde de 1 px degradado a 135°: color de acento de tarjeta en la esquina superior izquierda, transparente entre el 40 % y el 65 % y gris de borde en la esquina inferior derecha. El borde MUST NOT capturar clics ni el paso del ratón.

#### Scenario: Aspecto de un panel
- **WHEN** se muestra el panel Top proyectos
- **THEN** tiene esquinas rectas, deja ver el fondo de la ventana y su borde pasa del acento arriba a la izquierda al gris abajo a la derecha

#### Scenario: El borde no bloquea la interacción
- **WHEN** el usuario pulsa un enlace o una fila situada junto al borde de una tarjeta
- **THEN** la pulsación llega al enlace o a la fila

### Requirement: Color de arranque del borde de tarjeta
El color con el que arranca el borde degradado SHALL exponerse como un token propio de tema, con naranja `#f2a33a` en modo oscuro y celeste `#20b0f0` en modo claro.

#### Scenario: Cambio de tema
- **WHEN** el usuario cambia de tema oscuro a claro
- **THEN** el borde de las tarjetas pasa de arrancar en naranja `#f2a33a` a arrancar en celeste `#20b0f0`

### Requirement: Líneas internas finas
Las líneas internas de las tarjetas (separadores de filas de tabla, cabeceras de tabla y nota al pie de panel) SHALL tener 0,5 px de grosor.

#### Scenario: Tabla de un panel
- **WHEN** se muestra una tabla dentro de un panel
- **THEN** su cabecera y los separadores entre filas son líneas de 0,5 px

### Requirement: Elementos rectos dentro de las tarjetas
Los chips, las pestañas y controles segmentados, las barras de medidor y los avisos dentro de tarjetas SHALL tener esquinas rectas. Los botones, desplegables y campos de texto SHALL tener también esquinas rectas.

#### Scenario: Barra de medidor
- **WHEN** se muestra la barra de presupuesto o de reparto en una tarjeta
- **THEN** la pista y el relleno tienen esquinas rectas

#### Scenario: Aviso en tarjeta
- **WHEN** la tarjeta Top modelos muestra el aviso de modelos sin precio
- **THEN** el aviso tiene esquinas rectas

### Requirement: Colores de gráficos intactos
El rediseño MUST NOT cambiar los colores de gráficos y series (agentes, actividades, modelos, paletas categóricas y series) en ninguno de los dos modos.

#### Scenario: Gasto diario por agente
- **WHEN** se muestra el gasto diario apilado por agente tras el rediseño
- **THEN** cada agente conserva el mismo color que antes del rediseño en el modo activo

### Requirement: Columnas y leyendas de gráficos rectas
Las columnas de los gráficos de columnas, tanto apiladas como simples, SHALL tener esquinas rectas en todos sus segmentos. Las muestras de color de leyendas y listas de los gráficos SHALL ser cuadrados de esquinas rectas. Los indicadores circulares (estado, en vivo y logotipo) y los extremos de las líneas de los gráficos MUST NOT cambiar de forma.

#### Scenario: Columna apilada
- **WHEN** se muestra una columna del gasto diario apilado por agente
- **THEN** su segmento superior, su segmento inferior y, si es el único, ese segmento tienen esquinas rectas

#### Scenario: Columna simple
- **WHEN** se muestra un gráfico de columnas de una sola serie
- **THEN** cada columna tiene esquinas rectas

#### Scenario: Leyenda de un gráfico
- **WHEN** un gráfico o una lista de reparto muestra su leyenda con muestras de color
- **THEN** cada muestra es un cuadrado de esquinas rectas

#### Scenario: Indicadores circulares
- **WHEN** se muestran el punto de estado de una sesión o el indicador en vivo
- **THEN** siguen siendo círculos

### Requirement: Elementos de interfaz rectos
Las etiquetas e insignias (atajos de teclado, versión, «nuevo», etiquetas de origen y de comparación, indicador «En vivo»), las entradas de navegación y de menú, las opciones de desplegable y de buscador rápido, los campos de precio y de rango, las filas seleccionables, la miniatura de tema, el anillo de foco y el interruptor (pista y pulgar) SHALL tener esquinas rectas. El logotipo y los puntos de estado y en vivo MUST seguir siendo círculos.

#### Scenario: Barra superior
- **WHEN** se muestra la barra superior con los desplegables de agentes y proyectos, el buscador con su atajo, el botón de exportar y el indicador «En vivo»
- **THEN** todos tienen esquinas rectas y el punto del indicador sigue siendo un círculo

#### Scenario: Interruptor
- **WHEN** se muestra el interruptor «Comparar con periodo anterior»
- **THEN** su pista es un rectángulo y su pulgar un cuadrado, ambos de esquinas rectas

#### Scenario: Foco con teclado
- **WHEN** el usuario enfoca un control con el teclado
- **THEN** el anillo de foco tiene esquinas rectas

### Requirement: Ventanas flotantes rectas
Las ventanas flotantes (menús emergentes, diálogos, buscador rápido, listas de desplegable y tooltips) SHALL tener esquinas rectas y MUST conservar su sombra.

#### Scenario: Diálogo de ajustes
- **WHEN** se abre el diálogo de ajustes
- **THEN** tiene esquinas rectas

#### Scenario: Lista de un desplegable
- **WHEN** se abre la lista de opciones de un desplegable
- **THEN** la lista tiene esquinas rectas y sigue proyectando sombra
