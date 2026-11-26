## ADDED Requirements

### Requirement: Columnas y leyendas de gráficos rectas
Las columnas de los gráficos de columnas, tanto apiladas como simples, SHALL tener esquinas rectas en todos sus segmentos. Las muestras de color de leyendas y listas de los gráficos SHALL ser cuadrados de esquinas rectas. Los indicadores circulares (estado, en vivo, logotipo e interruptor) y los extremos de las líneas de los gráficos MUST NOT cambiar de forma.

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
