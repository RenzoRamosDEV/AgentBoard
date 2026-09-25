# Spec Delta

## Purpose

Responde de un vistazo cuánto se gasta, a qué ritmo y en qué (proyectos, ramas, modelos, herramientas), siempre respetando los filtros activos.

## ADDED Requirements

### Requirement: KPIs
El dashboard SHALL mostrar para el filtro activo: coste total (USD), llamadas, sesiones, cache hit (`cache_read / (input + cache_read + cache_write)`), ahorro por caché (`cache_read × (precio entrada − precio lectura caché)`) y burn rate (coste de los últimos 60 minutos, en USD/h).

#### Scenario: Cache hit
- **WHEN** en el periodo hay 100 tokens de entrada, 800 de lectura de caché y 100 de escritura
- **THEN** el cache hit mostrado es 80 %

#### Scenario: Burn rate sin actividad reciente
- **WHEN** no hay llamadas en los últimos 60 minutos
- **THEN** el burn rate es 0 USD/h

### Requirement: Gasto del mes
El panel SHALL mostrar el coste acumulado por día del mes en curso, la proyección a fin de mes (`acumulado / días transcurridos × días del mes`) y, si hay presupuesto mensual, su línea y el porcentaje consumido. Este panel ignora el periodo seleccionado pero respeta agentes y proyectos.

#### Scenario: Proyección
- **WHEN** el día 10 de un mes de 30 días el acumulado es 20 USD
- **THEN** la proyección es 60 USD

#### Scenario: Presupuesto superado
- **WHEN** el presupuesto es 50 USD y la proyección 60 USD
- **THEN** el panel indica que la proyección supera el presupuesto

### Requirement: Coste por proyecto y por rama
El dashboard SHALL mostrar el coste por proyecto ordenado de mayor a menor; con exactamente un proyecto seleccionado, SHALL mostrar el coste por rama git.

#### Scenario: Un proyecto elegido
- **WHEN** el filtro incluye solo el proyecto "web"
- **THEN** el panel muestra el coste por rama de "web"

### Requirement: Modelos
El dashboard SHALL mostrar por modelo: coste, llamadas y cache hit, y marcar los modelos sin precio conocido.

#### Scenario: Modelo sin precio
- **WHEN** hay llamadas de un modelo sin precio
- **THEN** aparece en la lista con coste 0 y la marca "sin precio"

### Requirement: Herramientas y comandos
El dashboard SHALL mostrar el ranking de herramientas por número de usos con su tasa de error, y el ranking de comandos de shell agrupados por su primera palabra.

#### Scenario: Comandos agrupados
- **WHEN** se ejecutaron `git status`, `git diff` y `npm test`
- **THEN** el ranking muestra `git` con 2 usos y `npm` con 1

### Requirement: Actualización tras la importación
Los paneles SHALL refrescarse cuando termine una importación.

#### Scenario: Escaneo inicial termina
- **WHEN** el escaneo inicial acaba mientras el dashboard está abierto
- **THEN** las cifras se actualizan sin recargar la app
