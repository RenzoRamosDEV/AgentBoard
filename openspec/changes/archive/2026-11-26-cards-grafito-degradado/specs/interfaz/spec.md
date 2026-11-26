# Spec Delta

## ADDED Requirements

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
Los chips, las pestañas y controles segmentados, las barras de medidor y los avisos dentro de tarjetas SHALL tener esquinas rectas. Los botones, desplegables y campos de texto MAY conservar su radio de esquina.

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

## MODIFIED Requirements

### Requirement: Consistencia de tamaños
Todos los controles interactivos (botones, botones de filtro, desplegables, campos de texto, buscadores y controles segmentados) SHALL compartir altura y tamaño de letra, salvo los campos dentro de tablas, que usan una variante compacta común. Los botones, botones de filtro, desplegables, campos de texto y buscadores SHALL compartir además radio de esquina; los controles segmentados SHALL tener esquinas rectas, como las tarjetas. Ningún control ni etiqueta SHALL partirse en dos líneas.

#### Scenario: Barra de herramientas
- **WHEN** una barra combina un buscador, un botón y un control segmentado
- **THEN** los tres tienen la misma altura

#### Scenario: Radio de los controles
- **WHEN** una barra combina un botón de filtro y un control segmentado
- **THEN** el botón de filtro conserva el radio común de los controles y el control segmentado tiene esquinas rectas
