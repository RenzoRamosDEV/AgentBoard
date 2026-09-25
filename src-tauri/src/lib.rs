//! Núcleo de AgentBoard: lee logs de agentes, los guarda en SQLite y los sirve a la UI.

pub mod commands;
pub mod db;
pub mod ingest;
pub mod insights;
pub mod pricing;
pub mod providers;
pub mod queries;
pub mod settings;

use commands::AppState;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let path = db::default_db_path()?;
            app.manage(AppState { db: Mutex::new(db::open(&path)?), db_path: path.clone() });

            // Escaneo inicial en segundo plano con su propia conexión.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let result = db::open(&path).and_then(|mut conn| ingest::scan_all(&mut conn, &providers::all()));
                match result {
                    Ok(stats) => {
                        let _ = handle.emit("ingest://done", stats);
                    }
                    Err(e) => eprintln!("agentboard: falló el escaneo inicial: {e:#}"),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_summary,
            commands::get_timeseries,
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
