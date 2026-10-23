//! Comandos Tauri: envoltorios finos sobre `queries` y `settings`.

use crate::ingest::now_ms;
use crate::insights::{self, ActivityDay, ActivityReport};
use crate::queries::{
    self, AgentRow, BreakdownRow, DataInfo, Filter, Point, ProjectRow, SeriesPoint, Summary,
};
use crate::sessions::{self, SessionDetail, SessionList};
use crate::settings::{self, Settings};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// Base en memoria compartida entre la UI y la ingesta.
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub alerts: std::sync::Arc<crate::alerts::Alerts>,
}

type CmdResult<T> = Result<T, String>;

fn with_db<T>(
    state: &tauri::State<AppState>,
    f: impl FnOnce(&Connection) -> anyhow::Result<T>,
) -> CmdResult<T> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    f(&conn).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_summary", level = "info", skip_all)]
pub fn get_summary(state: tauri::State<AppState>, filter: Option<Filter>) -> CmdResult<Summary> {
    with_db(&state, |c| {
        queries::summary(c, &filter.unwrap_or_default(), now_ms())
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_timeseries", level = "info", skip_all)]
pub fn get_timeseries(
    state: tauri::State<AppState>,
    filter: Option<Filter>,
    bucket: String,
    tz: String,
) -> CmdResult<Vec<Point>> {
    with_db(&state, |c| {
        queries::timeseries(c, &filter.unwrap_or_default(), &bucket, &tz)
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_timeseries_by", level = "info", skip_all)]
pub fn get_timeseries_by(
    state: tauri::State<AppState>,
    filter: Option<Filter>,
    by: String,
    tz: String,
) -> CmdResult<Vec<SeriesPoint>> {
    with_db(&state, |c| {
        queries::timeseries_by(c, &filter.unwrap_or_default(), &by, &tz)
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_breakdown", level = "info", skip_all)]
pub fn get_breakdown(
    state: tauri::State<AppState>,
    filter: Option<Filter>,
    by: String,
) -> CmdResult<Vec<BreakdownRow>> {
    with_db(&state, |c| {
        queries::breakdown(c, &filter.unwrap_or_default(), &by)
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_activity", level = "info", skip_all)]
pub fn get_activity(
    state: tauri::State<AppState>,
    filter: Option<Filter>,
) -> CmdResult<ActivityReport> {
    with_db(&state, |c| {
        insights::activity(c, &filter.unwrap_or_default())
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_activity_daily", level = "info", skip_all)]
pub fn get_activity_daily(
    state: tauri::State<AppState>,
    filter: Option<Filter>,
    tz: String,
) -> CmdResult<Vec<ActivityDay>> {
    with_db(&state, |c| {
        insights::activity_daily(c, &filter.unwrap_or_default(), &tz)
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_agents", level = "info", skip_all)]
pub fn list_agents(
    state: tauri::State<AppState>,
    filter: Option<Filter>,
) -> CmdResult<Vec<AgentRow>> {
    with_db(&state, |c| {
        queries::list_agents(c, &filter.unwrap_or_default())
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_projects", level = "info", skip_all)]
pub fn list_projects(
    state: tauri::State<AppState>,
    filter: Option<Filter>,
) -> CmdResult<Vec<ProjectRow>> {
    with_db(&state, |c| {
        queries::list_projects(c, &filter.unwrap_or_default())
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_data_info", level = "info", skip_all)]
pub fn get_data_info(state: tauri::State<AppState>) -> CmdResult<DataInfo> {
    with_db(&state, queries::data_info)
}

#[tauri::command]
#[tracing::instrument(name = "cmd.export_data", level = "info", skip_all)]
pub fn export_data(
    state: tauri::State<AppState>,
    filter: Option<Filter>,
    format: String,
) -> CmdResult<String> {
    with_db(&state, |c| {
        queries::export(c, &filter.unwrap_or_default(), &format)
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_sessions", level = "info", skip_all)]
pub fn list_sessions(
    state: tauri::State<AppState>,
    filter: Option<Filter>,
    limit: Option<usize>,
) -> CmdResult<SessionList> {
    with_db(&state, |c| {
        sessions::list_sessions(c, &filter.unwrap_or_default(), limit)
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_project_summaries", level = "info", skip_all)]
pub fn list_project_summaries(
    state: tauri::State<AppState>,
    filter: Option<Filter>,
) -> CmdResult<Vec<sessions::ProjectSummary>> {
    with_db(&state, |c| {
        sessions::list_projects(c, &filter.unwrap_or_default())
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_project_detail", level = "info", skip_all)]
pub fn get_project_detail(
    state: tauri::State<AppState>,
    key: String,
    filter: Option<Filter>,
    tz: String,
) -> CmdResult<sessions::ProjectDetail> {
    with_db(&state, |c| {
        sessions::project_detail(c, &key, &filter.unwrap_or_default(), &tz)
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_session_detail", level = "info", skip_all)]
pub fn get_session_detail(state: tauri::State<AppState>, id: String) -> CmdResult<SessionDetail> {
    with_db(&state, |c| sessions::session_detail(c, &id))
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_prices", level = "info", skip_all)]
pub fn list_prices(
    state: tauri::State<AppState>,
    filter: Option<Filter>,
) -> CmdResult<Vec<crate::pricing::PriceRow>> {
    let overrides = settings::load().price_overrides;
    with_db(&state, |c| {
        crate::pricing::list_prices(c, &filter.unwrap_or_default(), &overrides)
    })
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_settings", level = "info", skip_all)]
pub fn get_settings() -> CmdResult<Settings> {
    Ok(settings::load())
}

#[tauri::command]
#[tracing::instrument(name = "cmd.set_settings", level = "info", skip_all)]
pub fn set_settings(state: tauri::State<AppState>, settings: Settings) -> CmdResult<Settings> {
    settings::save(&settings).map_err(|e| format!("{e:#}"))?;
    state.alerts.reset(); // al cambiar el presupuesto se vuelven a permitir los avisos
    let saved = settings::load();
    // Los precios del usuario recalculan los costes al momento (sin releer logs).
    with_db(&state, |c| {
        crate::pricing::apply_overrides(c, &saved.price_overrides)
    })?;
    Ok(saved)
}
