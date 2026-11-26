## ADDED Requirements

### Requirement: Separación degradada del panel lateral
La línea vertical de 1 px que separa el panel lateral del contenido SHALL ser un degradado vertical: color de acento de tarjeta arriba, transparente entre el 40 % y el 65 % de la altura y gris de borde abajo, igual que el borde de las tarjetas. SHALL verse así con el panel desplegado, plegado a solo iconos y en ventanas estrechas, y MUST NOT dibujarse además una línea sólida que la duplique.

#### Scenario: Panel desplegado en tema oscuro
- **WHEN** se muestra el panel lateral desplegado en tema oscuro
- **THEN** la separación con el contenido arranca arriba en naranja `#f2a33a`, se desvanece hacia la mitad y termina abajo en el gris de borde

#### Scenario: Panel plegado o ventana estrecha
- **WHEN** el panel lateral está plegado a solo iconos o la ventana es tan estrecha que se pliega solo
- **THEN** la separación sigue siendo el mismo degradado vertical

### Requirement: Acento del elemento activo del panel lateral
El elemento activo del panel lateral SHALL mostrar a la izquierda una barra vertical recta de 2 px en el color de acento de tarjeta, a toda la altura del elemento, conservando su fondo gris y el texto en color primario. La barra MUST NOT desplazar el texto ni los iconos.

#### Scenario: Elemento activo en tema claro
- **WHEN** el usuario está en la vista By Model con el tema claro
- **THEN** el elemento «By Model» muestra a su izquierda una barra celeste `#20b0f0` de 2 px y su texto e icono quedan en la misma posición que en los elementos inactivos

#### Scenario: Elemento activo con el panel plegado
- **WHEN** el panel lateral está plegado a solo iconos
- **THEN** el elemento activo conserva la barra de 2 px a la izquierda y su icono sigue centrado
