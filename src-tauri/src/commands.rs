//! Comandos Tauri: envoltorios finos sobre `queries`.

use crate::queries::{self, Filter, Summary};
use rusqlite::Connection;
use std::sync::Mutex;

/// Conexión de solo lectura para la UI; la ingesta usa la suya propia (WAL).
pub struct AppState {
    pub db: Mutex<Connection>,
}

fn with_db<T>(state: &tauri::State<AppState>, f: impl FnOnce(&Connection) -> anyhow::Result<T>) -> Result<T, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    f(&conn).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn get_summary(state: tauri::State<AppState>, filter: Option<Filter>) -> Result<Summary, String> {
    with_db(&state, |c| queries::summary(c, &filter.unwrap_or_default()))
}
