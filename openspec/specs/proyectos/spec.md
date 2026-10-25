# proyectos Specification

## Purpose
Permite ver el uso de los agentes proyecto a proyecto y bajar de cada proyecto a sus sesiones.

## Requirements

### Requirement: Listado de proyectos
El sistema SHALL listar los proyectos (agrupados por la raíz de su repositorio) con al menos una llamada en el filtro, con: nombre y ruta, agentes que han trabajado en él, número de ramas, modelo principal (el de mayor coste), sesiones, tiempo activo (suma de la duración de sus sesiones), turnos, compactaciones, usos de herramientas con error, llamadas de subagentes, cache hit y coste, calculados con las llamadas del filtro. Las sesiones sin proyecto SHALL agruparse como «(sin proyecto)».

#### Scenario: Dos carpetas del mismo repo
- **WHEN** hay sesiones en `/w` y en un worktree `/w-feat` con la misma raíz de repositorio
- **THEN** aparecen como un solo proyecto con las sesiones de ambas

#### Scenario: Modelo principal
- **WHEN** un proyecto gasta 3 USD en claude-opus-4-5 y 1 USD en claude-haiku-4-5
- **THEN** su modelo principal es claude-opus-4-5

### Requirement: Cifras, búsqueda y vistas del listado
El apartado SHALL mostrar el coste medio por proyecto, el proyecto más caro, el tiempo activo total y cuántos proyectos tienen compactaciones; SHALL permitir buscar por proyecto, rama, modelo o agente, elegir una vista rápida (todos, más caros —por encima del p75—, con compactación, con subagentes) y ordenar por cualquier columna. La tabla MUST caber en el ancho de la ventana sin desplazamiento horizontal.

#### Scenario: Buscar por agente
- **WHEN** el usuario escribe «opencode»
- **THEN** solo quedan los proyectos en los que ha trabajado OpenCode

### Requirement: Detalle de proyecto
Al elegir un proyecto, el sistema SHALL mostrar sus cifras (coste, sesiones, turnos, tiempo activo, cache hit y compactaciones), el coste acumulado por día del periodo, el coste por actividad, los modelos y las ramas usadas, sus sesiones y la latencia p50/p95 de sus herramientas. Las sesiones del proyecto SHALL cargarse por páginas, con su total y con orden por columna en el servidor.

#### Scenario: Ir a una sesión
- **WHEN** el usuario pulsa una sesión en el detalle del proyecto
- **THEN** se abre el detalle de esa sesión, con enlace para volver al proyecto

#### Scenario: Proyecto con muchas sesiones
- **WHEN** un proyecto tiene 3000 sesiones en el periodo
- **THEN** el detalle muestra sus cifras y el número de sesiones con compactaciones calculados sobre todas ellas, pero recibe solo la primera página de sesiones

#### Scenario: Ordenar las sesiones del proyecto
- **WHEN** el usuario ordena las sesiones del proyecto por coste
- **THEN** la tabla muestra las sesiones más caras de todo el proyecto, no solo de las ya cargadas

### Requirement: Proyectos por MCP
El servidor MCP SHALL ofrecer `get_projects` (con el filtro común) y `get_project_detail` (por la clave del proyecto de `get_projects`, con el filtro común).

#### Scenario: Proyecto inexistente
- **WHEN** se pide el detalle de una clave que no existe
- **THEN** la herramienta devuelve un error controlado
