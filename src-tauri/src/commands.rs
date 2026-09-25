//! Comandos Tauri: envoltorios finos sobre `queries` y `settings`.

use crate::ingest::now_ms;
use crate::insights::{self, ActivityDay, ActivityReport};
use crate::queries::{self, AgentRow, BreakdownRow, DataInfo, Filter, Point, ProjectRow, SeriesPoint, Summary};
use crate::settings::{self, Settings};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// Base en memoria compartida entre la UI y la ingesta.
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
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
pub fn get_timeseries_by(state: tauri::State<AppState>, filter: Option<Filter>, by: String, tz_offset_min: i64) -> CmdResult<Vec<SeriesPoint>> {
    with_db(&state, |c| queries::timeseries_by(c, &filter.unwrap_or_default(), &by, tz_offset_min))
}

#[tauri::command]
pub fn get_breakdown(state: tauri::State<AppState>, filter: Option<Filter>, by: String) -> CmdResult<Vec<BreakdownRow>> {
    with_db(&state, |c| queries::breakdown(c, &filter.unwrap_or_default(), &by))
}

#[tauri::command]
pub fn get_activity(state: tauri::State<AppState>, filter: Option<Filter>) -> CmdResult<ActivityReport> {
    with_db(&state, |c| insights::activity(c, &filter.unwrap_or_default()))
}

#[tauri::command]
pub fn get_activity_daily(state: tauri::State<AppState>, filter: Option<Filter>, tz_offset_min: i64) -> CmdResult<Vec<ActivityDay>> {
    with_db(&state, |c| insights::activity_daily(c, &filter.unwrap_or_default(), tz_offset_min))
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
    with_db(&state, queries::data_info)
}

#[tauri::command]
pub fn export_data(state: tauri::State<AppState>, filter: Option<Filter>, format: String) -> CmdResult<String> {
    with_db(&state, |c| queries::export(c, &filter.unwrap_or_default(), &format))
}

#[tauri::command]
pub fn get_settings() -> CmdResult<Settings> {
    Ok(settings::load())
}

#[tauri::command]
pub fn set_settings(settings: Settings) -> CmdResult<Settings> {
    settings::save(&settings).map_err(|e| format!("{e:#}"))?;
    Ok(settings::load())
}
