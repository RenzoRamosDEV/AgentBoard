## MODIFIED Requirements

### Requirement: Elementos de interfaz rectos
Las etiquetas e insignias (atajos de teclado, versión, «nuevo», etiquetas de origen y de comparación, indicador «En vivo»), las entradas de navegación y de menú, las opciones de desplegable y de buscador rápido, los campos de precio y de rango, las filas seleccionables, la miniatura de tema (su lienzo y las tarjetas, barras y chips dibujados en ella), el anillo de foco y el interruptor (pista y pulgar) SHALL tener esquinas rectas. El logotipo (también el dibujado en la miniatura de tema) y los puntos de estado y en vivo MUST seguir siendo círculos.

#### Scenario: Barra superior
- **WHEN** se muestra la barra superior con los desplegables de agentes y proyectos, el buscador con su atajo, el botón de exportar y el indicador «En vivo»
- **THEN** todos tienen esquinas rectas y el punto del indicador sigue siendo un círculo

#### Scenario: Interruptor
- **WHEN** se muestra el interruptor «Comparar con periodo anterior»
- **THEN** su pista es un rectángulo y su pulgar un cuadrado, ambos de esquinas rectas

#### Scenario: Foco con teclado
- **WHEN** el usuario enfoca un control con el teclado
- **THEN** el anillo de foco tiene esquinas rectas

#### Scenario: Miniatura de tema
- **WHEN** se abre la pestaña Tema de Ajustes con las miniaturas Sistema, Claro y Oscuro
- **THEN** el lienzo de cada miniatura no tiene radio de borde y ninguna tarjeta, barra ni chip dibujado en ella tiene esquinas redondeadas, mientras el logotipo sigue siendo un círculo y los colores no cambian
