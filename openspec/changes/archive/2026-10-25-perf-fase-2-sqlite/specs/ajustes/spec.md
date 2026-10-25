# Spec Delta

## ADDED Requirements

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
