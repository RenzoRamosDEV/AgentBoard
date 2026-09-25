//! Núcleo de AgentBoard: lee logs de agentes, los guarda en SQLite y los sirve a la UI.

pub mod alerts;
pub mod commands;
pub mod db;
pub mod ingest;
pub mod insights;
pub mod pricing;
pub mod providers;
pub mod queries;
pub mod settings;
pub mod watcher;

use alerts::Alerts;
use commands::AppState;
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            db::remove_legacy_db();
            let db = Arc::new(Mutex::new(db::open_in_memory()?));
            let alerts = Alerts::new();
            app.manage(AppState {
                db: db.clone(),
                alerts: alerts.clone(),
            });

            // Bandeja del sistema: gasto del mes en el tooltip y menú Mostrar / Salir.
            let tray = build_tray(app.handle())?;

            // Refresca bandeja y avisos de presupuesto tras cada escaneo.
            let refresh_alerts = {
                let app = app.handle().clone();
                let db = db.clone();
                let alerts = alerts.clone();
                let tray = tray.clone();
                move || alerts.refresh(&app, &db, &tray)
            };

            // Escaneo inicial en segundo plano: lee los logs del ordenador a la base en memoria.
            let handle = app.handle().clone();
            let scan_db = db.clone();
            let after_scan = refresh_alerts.clone();
            std::thread::spawn(move || {
                let result = scan_db
                    .lock()
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .and_then(|mut conn| ingest::scan_all(&mut conn, &providers::all()));
                match result {
                    Ok(stats) => {
                        let _ = handle.emit("ingest://done", stats);
                        after_scan();
                    }
                    Err(e) => eprintln!("agentboard: falló el escaneo inicial: {e:#}"),
                }
            });

            // Vigilante en vivo: relee al vuelo cuando los agentes escriben en sus logs.
            let watch_handle = app.handle().clone();
            watcher::spawn(db.clone(), Arc::new(providers::all), move || {
                let _ = watch_handle.emit("ingest://done", ());
                refresh_alerts();
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Al cerrar la ventana, seguir en segundo plano (queda en la bandeja).
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_summary,
            commands::get_timeseries,
            commands::get_timeseries_by,
            commands::get_breakdown,
            commands::get_activity,
            commands::get_activity_daily,
            commands::list_agents,
            commands::list_projects,
            commands::get_data_info,
            commands::export_data,
            commands::get_settings,
            commands::set_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error al arrancar AgentBoard");
}

fn build_tray(app: &tauri::AppHandle) -> tauri::Result<TrayIcon> {
    let show = MenuItem::with_id(app, "show", "Mostrar AgentBoard", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("AgentBoard")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
