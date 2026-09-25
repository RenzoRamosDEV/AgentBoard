# Spec Delta

## MODIFIED Requirements

### Requirement: Presupuesto mensual
El usuario SHALL poder fijar un presupuesto mensual en USD (o dejarlo vacío) y el valor MUST persistir entre reinicios en `settings.json` dentro de la carpeta de configuración del usuario (`~/.config/agentboard/` en Linux).

#### Scenario: Guardar presupuesto
- **WHEN** el usuario guarda 50 como presupuesto y reinicia la app
- **THEN** el panel muestra el presupuesto de 50 USD leído del archivo

#### Scenario: Valor inválido
- **WHEN** el usuario introduce un número negativo
- **THEN** el valor se rechaza y el archivo mantiene el anterior
