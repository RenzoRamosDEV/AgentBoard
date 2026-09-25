# filtros Specification

## Purpose
Permite acotar todas las vistas del dashboard por agente, proyecto y periodo, partiendo siempre de una vista general de todo el historial.

## Requirements

### Requirement: Vista general por defecto
Al abrir la app, el sistema SHALL mostrar todos los agentes y todos los proyectos del periodo "30 días".

#### Scenario: Primer arranque del dashboard
- **WHEN** el usuario abre la app
- **THEN** ningún agente ni proyecto está excluido y el periodo seleccionado es "30 días"

### Requirement: Agentes y proyectos auto-detectados
La barra lateral SHALL listar los agentes y proyectos presentes en la base, cada uno con su coste en el periodo, un buscador por nombre y una casilla para ocultarlo.

#### Scenario: Ocultar un agente
- **WHEN** el usuario desmarca "Codex"
- **THEN** todos los paneles excluyen las llamadas de Codex

#### Scenario: Buscar un proyecto
- **WHEN** el usuario escribe "agent" en el buscador de proyectos
- **THEN** solo se listan los proyectos cuyo nombre contiene "agent", sin distinguir mayúsculas

### Requirement: Periodos
El sistema SHALL ofrecer los periodos Hoy, 7 días, 30 días, Mes (desde el día 1 del mes en curso), 6 meses, Todo y un rango personalizado, calculados en la zona horaria local.

#### Scenario: Periodo "Hoy"
- **WHEN** el usuario elige "Hoy" a las 15:00 hora local
- **THEN** se incluyen las llamadas desde las 00:00 locales de hoy

#### Scenario: Rango personalizado
- **WHEN** el usuario elige del 1 al 10 de septiembre
- **THEN** se incluyen las llamadas desde el 1 a las 00:00 hasta el 10 a las 23:59 locales

### Requirement: Información de datos
La barra lateral SHALL mostrar la fecha del primer registro, el tamaño en disco de la base y el número de archivos de log vigilados.

#### Scenario: Base con historial
- **WHEN** la base contiene llamadas desde el 3 de agosto
- **THEN** se muestra "Primer registro" con esa fecha
