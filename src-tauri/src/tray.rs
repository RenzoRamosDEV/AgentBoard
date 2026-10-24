//! Bandeja del sistema: menú con el gasto de hoy y las acciones «Panel de hoy»,
//! «Mostrar AgentBoard» y «Salir», y el panel emergente (ventana `panel`) con la UI
//! real de AgentBoard, al estilo de los applets: se abre pegado al cursor desde el
//! menú y se esconde al perder el foco (ver `on_window_event` en `lib.rs`). El menú
//! se reconstruye tras cada escaneo, que es cuando cambian los datos.

use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIcon;
use tauri::{Emitter, Manager, Wry};

/// Etiqueta de la ventana del panel emergente.
pub const PANEL: &str = "panel";

/// «Hoy: $12.34», la línea informativa del menú.
pub fn today_line(today_usd: f64) -> String {
    format!("Hoy: ${today_usd:.2}")
}

/// Reconstruye el menú de la bandeja con el gasto de hoy al día.
pub fn refresh(app: &tauri::AppHandle, readers: &crate::db::ReadPool, tray: &TrayIcon) {
    let Ok(today) = readers.with(|conn| Ok(crate::alerts::today_spent(conn).unwrap_or(0.0))) else {
        return;
    };
    match build_menu(app, &today_line(today)) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => tracing::warn!("agentboard: no se pudo reconstruir el menú de la bandeja: {e}"),
    }
}

/// Menú de la bandeja: el gasto de hoy (informativo) y las acciones de siempre,
/// con los mismos ids que maneja `build_tray` («panel», «show», «quit»).
fn build_menu(app: &tauri::AppHandle, today: &str) -> tauri::Result<Menu<Wry>> {
    let today = MenuItem::new(app, today, false, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let panel = MenuItem::with_id(app, "panel", "Panel de hoy", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Mostrar AgentBoard", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let items: [&dyn IsMenuItem<Wry>; 5] = [&today, &sep, &panel, &show, &quit];
    Menu::with_items(app, &items)
}

/// Enseña la ventana principal y esconde el panel si estaba abierto. También es el
/// comando `show_main` que invoca el propio panel («Abrir AgentBoard», sesiones).
#[tauri::command]
pub fn show_main(app: tauri::AppHandle) {
    if let Some(panel) = app.get_webview_window(PANEL) {
        let _ = panel.hide();
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Abre el panel emergente junto al cursor (el clic viene del menú de la bandeja)
/// y lo enfoca; al perder el foco se esconde solo.
pub fn show_panel(app: &tauri::AppHandle) {
    let Some(panel) = app.get_webview_window(PANEL) else {
        return;
    };
    position_panel(app, &panel);
    let _ = panel.show();
    let _ = panel.set_focus();
    // El panel se refresca nada más abrirse, sin esperar a su recarga periódica.
    let _ = app.emit_to(PANEL, "panel://shown", ());
}

/// Coloca el panel pegado al cursor sin salirse de su monitor. Sin cursor (Wayland),
/// arriba a la derecha del monitor primario; sin monitor, se queda donde esté.
fn position_panel(app: &tauri::AppHandle, panel: &tauri::WebviewWindow) {
    const MARGIN: i32 = 12;
    let (w, h) = panel
        .outer_size()
        .map(|s| (s.width as i32, s.height as i32))
        .unwrap_or((360, 500));
    let pos = if let Ok(cursor) = app.cursor_position() {
        let (cx, cy) = (cursor.x as i32, cursor.y as i32);
        if let Ok(Some(m)) = app.monitor_from_point(cursor.x, cursor.y) {
            // Si no cabe hacia abajo o la derecha, se pega al borde del monitor.
            let (mx, my) = (m.position().x, m.position().y);
            let (mw, mh) = (m.size().width as i32, m.size().height as i32);
            Some((cx.min(mx + mw - w).max(mx), cy.min(my + mh - h).max(my)))
        } else {
            Some(((cx - w).max(0), (cy - h).max(0)))
        }
    } else if let Ok(Some(m)) = app.primary_monitor() {
        let x = m.position().x + m.size().width as i32 - w - MARGIN;
        Some((x.max(m.position().x), m.position().y + MARGIN))
    } else {
        None
    };
    if let Some((x, y)) = pos {
        let _ = panel.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gasto_de_hoy_con_dos_decimales() {
        assert_eq!(today_line(12.339), "Hoy: $12.34");
        assert_eq!(today_line(0.0), "Hoy: $0.00");
    }
}
