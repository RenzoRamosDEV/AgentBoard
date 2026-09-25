# Spec Delta

## Purpose

Guarda en la base local las preferencias del usuario para que sobrevivan a reinicios y actualizaciones de la app.

## ADDED Requirements

### Requirement: Presupuesto mensual
El usuario SHALL poder fijar un presupuesto mensual en USD (o dejarlo vacío) y el valor MUST persistir entre reinicios.

#### Scenario: Guardar presupuesto
- **WHEN** el usuario guarda 50 como presupuesto y reinicia la app
- **THEN** el panel de gasto del mes muestra la línea de 50 USD

#### Scenario: Valor inválido
- **WHEN** el usuario introduce un número negativo
- **THEN** el valor se rechaza y se mantiene el anterior
