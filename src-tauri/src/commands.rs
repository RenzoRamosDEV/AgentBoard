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

/// Ejecuta `f` en el pool de hilos bloqueantes: ni el hilo principal ni el ejecutor async
/// esperan a la base ni al disco.
async fn run_blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

async fn with_db<T: Send + 'static>(
    db: Arc<Mutex<Connection>>,
    f: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    run_blocking(move || {
        let conn = db.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        f(&conn)
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_summary", level = "info", skip_all)]
pub async fn get_summary(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
) -> CmdResult<Summary> {
    with_db(state.db.clone(), move |c| {
        queries::summary(c, &filter.unwrap_or_default(), now_ms())
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_timeseries", level = "info", skip_all)]
pub async fn get_timeseries(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
    bucket: String,
    tz: String,
) -> CmdResult<Vec<Point>> {
    with_db(state.db.clone(), move |c| {
        queries::timeseries(c, &filter.unwrap_or_default(), &bucket, &tz)
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_timeseries_by", level = "info", skip_all)]
pub async fn get_timeseries_by(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
    by: String,
    tz: String,
) -> CmdResult<Vec<SeriesPoint>> {
    with_db(state.db.clone(), move |c| {
        queries::timeseries_by(c, &filter.unwrap_or_default(), &by, &tz)
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_breakdown", level = "info", skip_all)]
pub async fn get_breakdown(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
    by: String,
) -> CmdResult<Vec<BreakdownRow>> {
    with_db(state.db.clone(), move |c| {
        queries::breakdown(c, &filter.unwrap_or_default(), &by)
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_activity", level = "info", skip_all)]
pub async fn get_activity(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
) -> CmdResult<ActivityReport> {
    with_db(state.db.clone(), move |c| {
        insights::activity(c, &filter.unwrap_or_default())
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_activity_daily", level = "info", skip_all)]
pub async fn get_activity_daily(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
    tz: String,
) -> CmdResult<Vec<ActivityDay>> {
    with_db(state.db.clone(), move |c| {
        insights::activity_daily(c, &filter.unwrap_or_default(), &tz)
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_agents", level = "info", skip_all)]
pub async fn list_agents(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
) -> CmdResult<Vec<AgentRow>> {
    with_db(state.db.clone(), move |c| {
        queries::list_agents(c, &filter.unwrap_or_default())
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_projects", level = "info", skip_all)]
pub async fn list_projects(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
) -> CmdResult<Vec<ProjectRow>> {
    with_db(state.db.clone(), move |c| {
        queries::list_projects(c, &filter.unwrap_or_default())
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_data_info", level = "info", skip_all)]
pub async fn get_data_info(state: tauri::State<'_, AppState>) -> CmdResult<DataInfo> {
    with_db(state.db.clone(), queries::data_info).await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.export_data", level = "info", skip_all)]
pub async fn export_data(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
    format: String,
) -> CmdResult<String> {
    with_db(state.db.clone(), move |c| {
        queries::export(c, &filter.unwrap_or_default(), &format)
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_sessions", level = "info", skip_all)]
pub async fn list_sessions(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
    limit: Option<usize>,
) -> CmdResult<SessionList> {
    with_db(state.db.clone(), move |c| {
        sessions::list_sessions(c, &filter.unwrap_or_default(), limit)
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_project_summaries", level = "info", skip_all)]
pub async fn list_project_summaries(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
) -> CmdResult<Vec<sessions::ProjectSummary>> {
    with_db(state.db.clone(), move |c| {
        sessions::list_projects(c, &filter.unwrap_or_default())
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_project_detail", level = "info", skip_all)]
pub async fn get_project_detail(
    state: tauri::State<'_, AppState>,
    key: String,
    filter: Option<Filter>,
    tz: String,
) -> CmdResult<sessions::ProjectDetail> {
    with_db(state.db.clone(), move |c| {
        sessions::project_detail(c, &key, &filter.unwrap_or_default(), &tz)
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_session_detail", level = "info", skip_all)]
pub async fn get_session_detail(
    state: tauri::State<'_, AppState>,
    id: String,
) -> CmdResult<SessionDetail> {
    with_db(state.db.clone(), move |c| sessions::session_detail(c, &id)).await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_prices", level = "info", skip_all)]
pub async fn list_prices(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
) -> CmdResult<Vec<crate::pricing::PriceRow>> {
    let overrides = run_blocking(|| Ok(settings::load().price_overrides)).await?;
    with_db(state.db.clone(), move |c| {
        crate::pricing::list_prices(c, &filter.unwrap_or_default(), &overrides)
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_settings", level = "info", skip_all)]
pub async fn get_settings() -> CmdResult<Settings> {
    run_blocking(|| Ok(settings::load())).await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.set_settings", level = "info", skip_all)]
pub async fn set_settings(
    state: tauri::State<'_, AppState>,
    settings: Settings,
) -> CmdResult<Settings> {
    let saved = run_blocking(move || {
        settings::save(&settings)?;
        Ok(settings::load())
    })
    .await?;
    // Al cambiar el presupuesto se vuelven a permitir los avisos.
    state.alerts.reset();
    // Los precios del usuario recalculan los costes al momento (sin releer logs).
    let overrides = saved.price_overrides.clone();
    with_db(state.db.clone(), move |c| {
        crate::pricing::apply_overrides(c, &overrides)
    })
    .await?;
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    fn block<T>(f: impl std::future::Future<Output = T>) -> T {
        tauri::async_runtime::block_on(f)
    }

    #[test]
    fn run_blocking_ejecuta_fuera_del_hilo_llamador_y_devuelve_el_resultado() {
        let caller = std::thread::current().id();
        let (worker, n) = block(run_blocking(|| Ok((std::thread::current().id(), 42)))).unwrap();
        assert_ne!(worker, caller);
        assert_eq!(n, 42);
    }

    #[test]
    fn run_blocking_formatea_los_errores_con_su_contexto() {
        let err = block(run_blocking(|| -> anyhow::Result<()> {
            Err(anyhow::anyhow!("sin columna")).context("falló la consulta")
        }))
        .unwrap_err();
        assert_eq!(err, "falló la consulta: sin columna");
    }

    #[test]
    fn with_db_consulta_la_base_compartida() {
        let db = Arc::new(Mutex::new(crate::db::open_in_memory().unwrap()));
        let n = block(with_db(db, |c| {
            Ok(c.query_row("SELECT COUNT(*) FROM agents", [], |r| r.get::<_, i64>(0))?)
        }))
        .unwrap();
        assert_eq!(n, 0);
    }
}
