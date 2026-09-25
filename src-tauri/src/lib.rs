//! Núcleo de AgentBoard: lee logs de agentes, los guarda en SQLite y los sirve a la UI.

pub mod commands;
pub mod db;
pub mod ingest;
pub mod insights;
pub mod pricing;
pub mod providers;
pub mod queries;
pub mod settings;
pub mod watcher;

use commands::AppState;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            db::remove_legacy_db();
            let db = Arc::new(Mutex::new(db::open_in_memory()?));
            app.manage(AppState { db: db.clone() });

            // Escaneo inicial en segundo plano: lee los logs del ordenador a la base en memoria.
            let handle = app.handle().clone();
            let scan_db = db.clone();
            std::thread::spawn(move || {
                let result = scan_db
                    .lock()
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .and_then(|mut conn| ingest::scan_all(&mut conn, &providers::all()));
                match result {
                    Ok(stats) => {
                        let _ = handle.emit("ingest://done", stats);
                    }
                    Err(e) => eprintln!("agentboard: falló el escaneo inicial: {e:#}"),
                }
            });

            // Vigilante en vivo: relee al vuelo cuando los agentes escriben en sus logs.
            let watch_handle = app.handle().clone();
            watcher::spawn(db.clone(), move || {
                let _ = watch_handle.emit("ingest://done", ());
            });
            Ok(())
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
            commands::get_settings,
            commands::set_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error al arrancar AgentBoard");
}
