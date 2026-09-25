//! Comandos Tauri: envoltorios finos sobre `queries` y `settings`.

use crate::ingest::now_ms;
use crate::queries::{self, AgentRow, BreakdownRow, DataInfo, Filter, Point, ProjectRow, Summary};
use crate::settings::{self, Settings};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

/// Conexión para la UI; la ingesta usa la suya propia (WAL).
pub struct AppState {
    pub db: Mutex<Connection>,
    pub db_path: PathBuf,
}

type CmdResult<T> = Result<T, String>;

fn with_db<T>(state: &tauri::State<AppState>, f: impl FnOnce(&Connection) -> anyhow::Result<T>) -> CmdResult<T> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    f(&conn).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn get_summary(state: tauri::State<AppState>, filter: Option<Filter>) -> CmdResult<Summary> {
    with_db(&state, |c| queries::summary(c, &filter.unwrap_or_default(), now_ms()))
}

#[tauri::command]
pub fn get_timeseries(
    state: tauri::State<AppState>,
    filter: Option<Filter>,
    bucket: String,
    tz_offset_min: i64,
) -> CmdResult<Vec<Point>> {
    with_db(&state, |c| queries::timeseries(c, &filter.unwrap_or_default(), &bucket, tz_offset_min))
}

#[tauri::command]
pub fn get_breakdown(state: tauri::State<AppState>, filter: Option<Filter>, by: String) -> CmdResult<Vec<BreakdownRow>> {
    with_db(&state, |c| queries::breakdown(c, &filter.unwrap_or_default(), &by))
}

#[tauri::command]
pub fn list_agents(state: tauri::State<AppState>, filter: Option<Filter>) -> CmdResult<Vec<AgentRow>> {
    with_db(&state, |c| queries::list_agents(c, &filter.unwrap_or_default()))
}

#[tauri::command]
pub fn list_projects(state: tauri::State<AppState>, filter: Option<Filter>) -> CmdResult<Vec<ProjectRow>> {
    with_db(&state, |c| queries::list_projects(c, &filter.unwrap_or_default()))
}

#[tauri::command]
pub fn get_data_info(state: tauri::State<AppState>) -> CmdResult<DataInfo> {
    with_db(&state, |c| queries::data_info(c, &state.db_path))
}

#[tauri::command]
pub fn get_settings(state: tauri::State<AppState>) -> CmdResult<Settings> {
    with_db(&state, settings::load)
}

#[tauri::command]
pub fn set_settings(state: tauri::State<AppState>, settings: Settings) -> CmdResult<Settings> {
    with_db(&state, |c| {
        settings::save(c, &settings)?;
        settings::load(c)
    })
}
