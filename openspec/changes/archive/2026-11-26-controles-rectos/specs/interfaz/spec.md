## MODIFIED Requirements

### Requirement: Consistencia de tamaños
Todos los controles interactivos (botones, botones de filtro, desplegables, campos de texto, buscadores y controles segmentados) SHALL compartir altura y tamaño de letra, salvo los campos dentro de tablas, que usan una variante compacta común. Todos ellos SHALL compartir además radio de esquina 0, como las tarjetas. Ningún control ni etiqueta SHALL partirse en dos líneas.

#### Scenario: Barra de herramientas
- **WHEN** una barra combina un buscador, un botón y un control segmentado
- **THEN** los tres tienen la misma altura

#### Scenario: Radio de los controles
- **WHEN** una barra combina un botón de filtro, un buscador, un botón y un control segmentado
- **THEN** todos tienen esquinas rectas

### Requirement: Elementos rectos dentro de las tarjetas
Los chips, las pestañas y controles segmentados, las barras de medidor y los avisos dentro de tarjetas SHALL tener esquinas rectas. Los botones, desplegables y campos de texto SHALL tener también esquinas rectas.

#### Scenario: Barra de medidor
- **WHEN** se muestra la barra de presupuesto o de reparto en una tarjeta
- **THEN** la pista y el relleno tienen esquinas rectas

#### Scenario: Aviso en tarjeta
- **WHEN** la tarjeta Top modelos muestra el aviso de modelos sin precio
- **THEN** el aviso tiene esquinas rectas

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

## ADDED Requirements

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
