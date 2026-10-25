# ajustes Specification

## Purpose
Guarda en la base local las preferencias del usuario para que sobrevivan a reinicios y actualizaciones de la app.

## Requirements

### Requirement: Presupuesto mensual
El usuario SHALL poder fijar un presupuesto mensual en USD (o dejarlo vacío) y el valor MUST persistir entre reinicios en `settings.json` dentro de la carpeta de configuración del usuario (`~/.config/agentboard/` en Linux).

#### Scenario: Guardar presupuesto
- **WHEN** el usuario guarda 50 como presupuesto y reinicia la app
- **THEN** el panel muestra el presupuesto de 50 USD leído del archivo

#### Scenario: Valor inválido
- **WHEN** el usuario introduce un número negativo
- **THEN** el valor se rechaza y el archivo mantiene el anterior

### Requirement: Presupuesto diario
El usuario SHALL poder fijar un presupuesto diario en USD (o dejarlo vacío), comparado con el gasto del día local en curso de todos los agentes y proyectos; MUST persistir en `settings.json`.

#### Scenario: Aviso diario
- **WHEN** el presupuesto diario es 30 USD y el gasto de hoy llega a 24 USD
- **THEN** se notifica una vez que se ha llegado al 80 % del presupuesto diario

### Requirement: Presupuestos por proyecto y por agente
El usuario SHALL poder añadir y quitar presupuestos mensuales para un proyecto (identificado por la raíz de su repositorio) o para un agente, comparados con la proyección del mes de ese proyecto o agente. MUST persistir en `settings.json` y los importes MUST ser mayores que 0.

#### Scenario: Presupuesto de un agente
- **WHEN** Codex tiene un presupuesto de 60 USD y su proyección del mes es 58 USD
- **THEN** el apartado muestra la barra de Codex al 96 % y se notifica el 80 %

### Requirement: Avisos de todos los presupuestos
Cada presupuesto (mensual, diario, por proyecto y por agente) SHALL notificar como mucho una vez por sesión de la app al llegar al 80 % y otra al llegar al 100 %; al cambiar los presupuestos, los avisos se rearman.

#### Scenario: Sin repetir
- **WHEN** tras avisar del 80 % de un presupuesto se vuelve a releer los logs sin cambios en los ajustes
- **THEN** no se repite el aviso

### Requirement: Preferencias de avisos
El usuario SHALL poder activar o desactivar por separado los avisos del 80 % y del 100 % de los presupuestos, y elegir si la bandeja muestra el gasto de hoy además del del mes. MUST persistir en `settings.json`; por defecto ambos avisos están activos y la bandeja muestra solo el mes.

#### Scenario: Sin aviso del 80 %
- **WHEN** el usuario desactiva el aviso del 80 % y un presupuesto llega al 85 %
- **THEN** no se notifica

#### Scenario: Gasto de hoy en la bandeja
- **WHEN** el usuario activa «Mostrar el gasto de hoy en la bandeja»
- **THEN** el texto de la bandeja incluye el gasto del día

### Requirement: Datos solo en memoria
El usuario SHALL poder activar en Ajustes la opción «Solo memoria», que MUST persistir en `settings.json` y está desactivada por defecto. Con la opción activa, los datos MUST vivir solo en memoria y la caché en disco MUST borrarse; al desactivarla, la caché se vuelve a crear en disco. El cambio MUST aplicarse sin reiniciar la app.

#### Scenario: Activar solo memoria
- **WHEN** el usuario activa «Solo memoria» con una caché en disco existente
- **THEN** la caché y sus archivos auxiliares se borran, los datos se vuelven a leer de los logs a memoria y el panel sigue mostrando las mismas cifras

#### Scenario: Reinicio con solo memoria
- **WHEN** la app se reinicia con «Solo memoria» activa
- **THEN** no se crea ningún archivo de caché y los datos se leen de los logs

#### Scenario: Valor por defecto
- **WHEN** `settings.json` no tiene la opción
- **THEN** los datos se guardan en la caché en disco

### Requirement: Reconstruir la caché
Ajustes SHALL ofrecer una acción «Reconstruir caché» que borra los datos guardados y vuelve a leer todos los logs, tanto con la caché en disco como en modo solo memoria. Mientras se reconstruye, la interfaz MUST seguir respondiendo con los datos ya releídos y la acción MUST quedar deshabilitada hasta que termine.

#### Scenario: Reconstrucción
- **WHEN** el usuario pulsa «Reconstruir caché»
- **THEN** la caché se vacía, se releen todos los logs, la acción se habilita de nuevo al terminar y el panel se actualiza

#### Scenario: Textos traducidos
- **WHEN** la interfaz está en inglés, portugués o francés
- **THEN** la opción «Solo memoria», la acción «Reconstruir caché» y sus explicaciones se muestran en ese idioma
