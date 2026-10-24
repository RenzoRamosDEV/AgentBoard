//! Núcleo de AgentBoard: lee logs de agentes, los guarda en SQLite y los sirve a la UI.

pub mod alerts;
pub mod commands;
pub mod db;
pub mod derived;
pub mod findings;
pub mod ingest;
pub mod ingest_thread;
pub mod insights;
pub mod pricing;
pub mod providers;
pub mod queries;
pub mod result_cache;
pub mod sessions;
pub mod settings;
pub mod telemetry;
pub mod tray;
pub mod tz;
pub mod watcher;

use alerts::Alerts;
use commands::AppState;
use ingest_thread::{DataGeneration, IngestThread, Origen};
use std::sync::{Arc, RwLock};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{Emitter, Manager};

pub fn run() {
    telemetry::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            db::remove_legacy_db();
            let target = db::Target::shared_memory();
            let writer = db::open_writer(&target)?;
            // Precios fijados por el usuario en Ajustes, encima de los de por defecto.
            if let Err(e) = pricing::apply_overrides(&writer, &settings::load().price_overrides) {
                tracing::warn!("agentboard: no se pudieron aplicar los precios del usuario: {e:#}");
            }
            let readers: commands::Readers = Arc::new(RwLock::new(Arc::new(db::ReadPool::open(
                &target,
                db::ReadPool::SIZE,
            )?)));
            let generation = Arc::new(DataGeneration::default());
            let alerts = Alerts::new();

            // Bandeja del sistema: tooltip con el gasto del mes y menú con el de hoy.
            let tray = build_tray(app.handle())?;

            // Panel emergente de la bandeja, como un applet: oculto hasta que se pide
            // desde el menú, y se esconde solo al perder el foco.
            tauri::WebviewWindowBuilder::new(
                app,
                tray::PANEL,
                tauri::WebviewUrl::App("index.html?panel=1".into()),
            )
            .title("AgentBoard — Hoy")
            .inner_size(360.0, 500.0)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .build()?;

            // Refresca la bandeja (tooltip y menú) y los avisos de presupuesto tras cada escaneo.
            let refresh_alerts = {
                let app = app.handle().clone();
                let readers = readers.clone();
                let alerts = alerts.clone();
                let tray = tray.clone();
                move || {
                    if let Ok(pool) = commands::current(&readers) {
                        alerts.refresh(&app, &pool, &tray);
                        tray::refresh(&app, &pool, &tray);
                    }
                }
            };

            // Hilo de ingesta: escaneo inicial y relecturas del vigilante, fuera de la interfaz.
            let handle = app.handle().clone();
            let ingest = IngestThread::spawn(
                writer,
                providers::all(),
                generation.clone(),
                move |origen, stats| match origen {
                    Origen::EscaneoInicial => {
                        tracing::debug_span!("emit.ingest_done", origen = "escaneo_inicial")
                            .in_scope(|| handle.emit("ingest://done", stats))
                            .ok();
                        refresh_alerts();
                    }
                    Origen::Vigilante if stats.records > 0 => {
                        tracing::debug_span!("emit.ingest_done", origen = "vigilante")
                            .in_scope(|| handle.emit("ingest://done", ()))
                            .ok();
                        refresh_alerts();
                    }
                    Origen::Vigilante => {}
                },
            );
            ingest.request(Origen::EscaneoInicial);
            app.manage(AppState {
                readers,
                ingest: ingest.clone(),
                generation,
                results: Arc::new(result_cache::ResultCache::default()),
                alerts,
            });

            // Vigilante en vivo: pide relecturas cuando los agentes escriben en sus logs.
            watcher::spawn(Arc::new(providers::all), ingest);
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                // Al cerrar la ventana, seguir en segundo plano (queda en la bandeja).
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    let _ = window.hide();
                    api.prevent_close();
                }
                // El panel se esconde solo al perder el foco, como los applets.
                tauri::WindowEvent::Focused(false) if window.label() == tray::PANEL => {
                    let _ = window.hide();
                }
                _ => {}
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
            commands::list_sessions,
            commands::get_session_detail,
            commands::list_project_summaries,
            commands::get_project_detail,
            commands::list_prices,
            commands::get_settings,
            commands::set_settings,
            tray::show_main,
        ])
        .run(tauri::generate_context!())
        .expect("error al arrancar AgentBoard");
}

fn build_tray(app: &tauri::AppHandle) -> tauri::Result<TrayIcon> {
    // Menú de arranque; tras el primer escaneo `tray::refresh` lo reconstruye con datos.
    let panel = MenuItem::with_id(app, "panel", "Panel de hoy", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Mostrar AgentBoard", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&panel, &show, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("AgentBoard")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "panel" => tray::show_panel(app),
            "show" => tray::show_main(app.clone()),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Al pulsar el icono se abre el panel directamente, como un applet.
            // (En appindicator el clic no llega; queda «Panel de hoy» en el menú.)
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                tray::show_panel(tray.app_handle());
            }
        })
        .build(app)
}
