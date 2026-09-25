# precios-modelos Specification

## Purpose
Calcula el coste en USD de cada llamada a partir de los tokens guardados y del precio vigente del modelo en la fecha de la llamada, sin necesidad de reimportar si cambian los precios.

## Requirements

### Requirement: Coste por llamada
El coste de una llamada SHALL ser `(input × p.input + output × p.output + cache_read × p.cache_read + cache_write × p.cache_write) / 1 000 000`, usando el precio del modelo con `valid_from` más reciente no posterior a la llamada.

#### Scenario: Llamada con precio conocido
- **WHEN** un modelo cuesta 3 USD de entrada y 15 USD de salida por millón, y una llamada usa 1 000 000 de entrada y 100 000 de salida
- **THEN** su coste es 4,5 USD

### Requirement: Precios iniciales
La app SHALL incluir una tabla de precios de los modelos Claude, OpenAI y Gemini habituales, que se carga en la base si no hay precios.

#### Scenario: Base recién creada
- **WHEN** se crea la base
- **THEN** la tabla de precios contiene al menos los modelos Claude actuales

### Requirement: Modelo sin precio
Una llamada cuyo modelo no tenga precio SHALL contarse en llamadas y tokens con coste 0, y el modelo MUST poder identificarse como "sin precio".

#### Scenario: Modelo desconocido
- **WHEN** se importa una llamada del modelo `modelo-inventado`
- **THEN** aparece en el recuento de llamadas con coste 0

### Requirement: Recalcular sin reimportar
Cambiar la tabla de precios SHALL cambiar los costes mostrados sin volver a leer los logs.

#### Scenario: Actualización de precios
- **WHEN** se añade un precio nuevo con `valid_from` anterior a las llamadas
- **THEN** el resumen refleja el nuevo coste en la siguiente consulta

### Requirement: Coste reportado por el agente
Si una llamada trae un coste reportado por el agente y su modelo no tiene precio en la tabla, el sistema SHALL usar ese coste y MUST NOT marcar el modelo como "sin precio".

#### Scenario: Modelo solo conocido por OpenCode
- **WHEN** una llamada del modelo `big-pickle` trae coste reportado 0,0123 USD y no hay precio en la tabla
- **THEN** su coste es 0,0123 USD y el modelo no aparece como "sin precio"
