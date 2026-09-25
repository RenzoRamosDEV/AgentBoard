//! Núcleo de AgentBurn: lee logs de agentes, los guarda en SQLite y los sirve a la UI.

pub mod commands;
pub mod db;
pub mod ingest;
pub mod pricing;
pub mod providers;
pub mod queries;

use commands::AppState;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let path = db::default_db_path()?;
            app.manage(AppState { db: Mutex::new(db::open(&path)?) });

            // Escaneo inicial en segundo plano con su propia conexión.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let result = db::open(&path).and_then(|mut conn| ingest::scan_all(&mut conn, &providers::all()));
                match result {
                    Ok(stats) => {
                        let _ = handle.emit("ingest://done", stats);
                    }
                    Err(e) => eprintln!("agentburn: falló el escaneo inicial: {e:#}"),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::get_summary])
        .run(tauri::generate_context!())
        .expect("error al arrancar AgentBurn");
}
