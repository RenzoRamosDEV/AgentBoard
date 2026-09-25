# Funcionalidad

AgentBoard organiza toda la actividad en un panel lateral y una serie de apartados. Cada
apartado tiene su tabla y su gráfico, y una vista ampliada al pulsarlo. Todo respeta los
filtros activos (periodo, agentes y proyectos).

## Apartados

| Apartado | Qué responde |
| --- | --- |
| **Resumen** | Coste total, llamadas, sesiones, cache hit, ahorro por caché, burn rate y gasto del mes con su proyección. |
| **Daily Activity** | Cuánto se gasta cada día. |
| **By Agent** | Qué agente se usa más y cuánto cuesta cada uno. |
| **By Project** | Coste por proyecto (y por rama al elegir un proyecto), con el *overhead* de contexto. |
| **By Activity** | Reparto por tipo de actividad: coding, testing, debugging, exploración, conversación, etc. |
| **By Model** | Coste, cache hit y llamadas por modelo. |
| **Tools** | Uso y porcentaje de error por herramienta nativa. |
| **Shell Commands** | Comandos de shell más ejecutados. |
| **Skills & Agents** | Skills y subagentes invocados. |
| **MCP Servers** | Servidores MCP usados y su actividad. |
| **Claude Agent Types** | Tipos de subagente de Claude Code y su coste. |

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
- **Avisos de presupuesto.** Notificación nativa al llegar al 80 % y al 100 % de la proyección.
- **Exportar.** Las llamadas del filtro activo a CSV o JSON desde Ajustes.
- **Temas e idiomas.** Tema claro, oscuro o del sistema; interfaz en español, inglés, portugués
  o francés (o el idioma del sistema).
- **Panel colapsable.** El panel lateral se pliega a solo iconos para ganar espacio.

## De dónde salen los datos

Un agente aparece si está instalado (ejecutable en el PATH o su carpeta de configuración),
aunque todavía no tenga sesiones. Al arrancar se leen todos los logs a una base en memoria;
cada llamada se identifica por su `message_id`, de modo que releer nunca duplica. El texto de
los prompts y las respuestas no se guarda en ningún sitio.
