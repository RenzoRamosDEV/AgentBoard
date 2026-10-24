# Funcionalidad

AgentBoard organiza toda la actividad en un panel lateral con los apartados agrupados
(*Coste*, *Trabajo*, *Herramientas* y *Configuración*) y una barra superior con los filtros:
periodo, agentes, proyectos y **comparar con el periodo anterior**. Cada apartado tiene su tabla
y su gráfico, y una vista ampliada al pulsarlo. Todo respeta los filtros activos.

## Apartados

| Apartado | Qué responde |
| --- | --- |
| **Resumen** | KPIs (coste, sesiones, cache hit, ahorro, burn rate, gasto del mes) con su variación, gasto diario por agente con la línea del periodo anterior, «Hoy en directo» y top de proyectos, modelos y actividades. |
| **Daily Activity** | Cuánto se gasta cada día. |
| **By Agent** | Qué agente se usa más y cuánto cuesta cada uno. |
| **By Project** | Coste por proyecto (y por rama al elegir un proyecto), con el *overhead* de contexto. |
| **Proyectos** | Una fila por proyecto (agrupando worktrees): agentes, ramas, modelo principal, sesiones, tiempo activo, turnos, compactaciones y coste, con búsqueda y vistas rápidas. Su detalle muestra el coste acumulado por día, actividades, modelos, ramas, latencia de herramientas y sus sesiones; cada sesión abre su propio detalle (coste acumulado con las compactaciones, turnos y latencia). |
| **By Activity** | Reparto por tipo de actividad: coding, testing, debugging, exploración, conversación, etc. |
| **By Model** | Coste, cache hit y llamadas por modelo. |
| **Tools** | Uso y porcentaje de error por herramienta nativa. |
| **Shell Commands** | Comandos de shell más ejecutados. |
| **Skills & Agents** | Skills y subagentes invocados. |
| **MCP Servers** | Servidores MCP usados y su actividad. |
| **Agent Types** | Subagentes de todos los agentes (Claude Code, Codex, Gemini, OpenCode, Cursor…), por agente y tipo, con su coste. |
| **Precios y presupuestos** | Precios por modelo editables (origen: por defecto, editado, coste del agente o falta precio) y presupuestos mensual, diario y por proyecto o agente con su progreso. |

## Hoy en directo

Junto al gasto diario, la portada muestra siempre lo de hoy (sea cual sea el periodo, con los
filtros de agentes y proyectos): el gasto desde las 00:00 y su variación frente a ayer hasta la
misma hora, el ritmo de la última hora, el gasto por hora y la sesión activa (o la última de
hoy) con enlace a su detalle. Se actualiza con cada relectura de los logs.

## Avisos para agentes (MCP)

La herramienta MCP `get_insights` calcula avisos sobre el filtro, del más grave al menos
(importante, aviso, información y lo que va bien), con su texto en español:

| Aviso | Salta cuando |
| --- | --- |
| Compactaciones | Hay sesiones con 3 o más compactaciones de contexto |
| Modelo caro en tareas sencillas | Opus o Fable en exploración/conversación, con ahorro estimado > 1 USD usando Sonnet 5 |
| Herramienta que falla | ≥ 20 usos y > 10 % de error |
| Servidor MCP que falla | ≥ 10 usos y > 10 % de error |
| Comando que falla | ≥ 5 errores y ≥ 5 % (sin contar auxiliares como `echo`, `cd` o `grep`) |
| Modelos sin precio | Algún modelo del periodo no tiene precio |
| Cache hit bajo | Un modelo con ≥ 100 llamadas por debajo del 70 % |
| 1-shot bajo / bueno | < 80 % o ≥ 95 %, con ≥ 20 turnos con ediciones |
| Concentración | Un proyecto con ≥ 50 % del gasto, o un modelo con ≥ 70 % |
| Sesión más cara | Supone ≥ 20 % del gasto o más de 5 USD |
| Subagentes | Suman ≥ 20 % del gasto |
| Ritmo alto | La última hora cuesta más del doble de la media por hora activa |
| Fuera de horario | ≥ 30 % del gasto antes de las 9 o desde las 19 h |
| Pico de gasto | Un día > 2× la media y > media + 2σ (con ≥ 5 días de actividad) |
| Caída del cache hit / coste por sesión al alza | Frente al periodo anterior: > 3 puntos / > 30 % |
| Agentes sin uso | Instalados pero sin llamadas en el periodo |
| Ahorro por caché | La caché ahorró más de lo que se gastó |

## Métricas destacadas

- **Cache hit** — proporción de tokens servidos desde caché sobre el total de entrada.
- **Ahorro por caché** — estimación de lo que habría costado ese contexto a precio completo.
- **Burn rate** — coste de los últimos 60 minutos, en USD/hora.
- **Proyección del mes** — gasto acumulado extrapolado a fin de mes, con aviso si supera el
  presupuesto configurado.

## Funciones de escritorio

- **En vivo.** Un vigilante de archivos relee los logs al vuelo mientras la app está abierta;
  el panel se actualiza sin recargar.
- **Bandeja del sistema.** El icono muestra el gasto del mes en el tooltip y un menú
  Mostrar / Salir; al cerrar la ventana la app sigue en segundo plano.
- **Avisos de presupuesto.** Notificación nativa al llegar al 80 % y al 100 % de cada
  presupuesto: mensual y por proyecto o agente (sobre la proyección del mes) y diario (sobre el
  gasto de hoy).
- **Precios propios.** Los precios editados se guardan en `settings.json` y recalculan todos los
  costes al momento, en la app y en el servidor MCP.
- **Exportar.** Las llamadas del filtro activo a CSV o JSON desde la barra superior o Ajustes.
- **Temas e idiomas.** Tema claro, oscuro o del sistema; interfaz en español, inglés, portugués
  o francés (o el idioma del sistema).
- **Buscador Ctrl+K.** Salta a cualquier apartado, deja solo un proyecto o abre una sesión reciente.
- **Preferencias de avisos.** Activar o desactivar los avisos del 80 % y del 100 %, y mostrar el
  gasto de hoy en la bandeja.
- **Panel colapsable.** El panel lateral se pliega a solo iconos para ganar espacio.
- **Datos (Ajustes → Datos).** **«Solo memoria»** (desactivado por defecto) deja de guardar la
  caché en disco y la borra: al abrir la app se vuelven a leer todos los logs. **«Reconstruir
  caché»** borra los datos guardados y relee todos los logs; el botón queda deshabilitado hasta
  que termina y el panel se actualiza solo. Ambas cosas se aplican sin reiniciar la app.

## De dónde salen los datos

Un agente aparece si está instalado (ejecutable en el PATH o su carpeta de configuración),
aunque todavía no tenga sesiones. La primera vez se leen todos los logs; después, la caché local
recuerda qué se leyó de cada archivo y al arrancar solo se lee lo nuevo. Cada llamada se
identifica por su `message_id`, de modo que releer nunca duplica, y si un log desaparece sus datos
dejan de contar. El texto de
los prompts y las respuestas no se guarda en ningún sitio.
