# Spec Delta

## ADDED Requirements

### Requirement: Coste reportado por el agente
Si una llamada trae un coste reportado por el agente y su modelo no tiene precio en la tabla, el sistema SHALL usar ese coste y MUST NOT marcar el modelo como "sin precio".

#### Scenario: Modelo solo conocido por OpenCode
- **WHEN** una llamada del modelo `big-pickle` trae coste reportado 0,0123 USD y no hay precio en la tabla
- **THEN** su coste es 0,0123 USD y el modelo no aparece como "sin precio"
