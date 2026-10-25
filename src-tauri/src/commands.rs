//! Comandos Tauri: envoltorios finos sobre `queries` y `settings`.

use crate::db::ReadPool;
use crate::ingest::now_ms;
use crate::ingest_thread::{DataGeneration, IngestHandle};
use crate::insights::{self, ActivityDay, ActivityReport};
use crate::queries::{
    self, AgentRow, BreakdownRow, DataInfo, Filter, Point, ProjectRow, SeriesPoint, Summary,
};
use crate::result_cache::{self, ResultCache};
use crate::sessions::{self, SessionDetail, SessionList};
use crate::settings::{self, Settings};
use rusqlite::Connection;
use std::sync::Arc;

/// Lectores que usa la interfaz; una reconstrucción de la caché sustituye el conjunto y las
/// consultas en curso terminan con el anterior.
pub type Readers = crate::db::SharedReaders;

/// Estado de la app: lectores de la base, el hilo de ingesta (único escritor), la caché de
/// resultados con la generación de datos que la invalida, y los avisos.
pub struct AppState {
    pub readers: Readers,
    pub ingest: IngestHandle,
    pub generation: Arc<DataGeneration>,
    pub results: Arc<ResultCache>,
    pub alerts: std::sync::Arc<crate::alerts::Alerts>,
}

/// El conjunto de lectores vigente.
pub fn current(readers: &Readers) -> anyhow::Result<Arc<ReadPool>> {
    Ok(readers
        .read()
        .map_err(|e| anyhow::anyhow!("lectores envenenados: {e}"))?
        .clone())
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

/// Como `with_reader`, pero a través de la caché de resultados con la clave `key`.
async fn cached<T: Clone + Send + Sync + 'static>(
    state: &AppState,
    key: String,
    f: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let (readers, results, generation) = (
        state.readers.clone(),
        state.results.clone(),
        state.generation.clone(),
    );
    let ingest = state.ingest.clone();
    run_blocking(move || {
        let value = results
            .get_or_compute(&generation, key, || current(&readers)?.with(f))
            .inspect_err(|e| ingest.report(e))?;
        Ok(T::clone(&value))
    })
    .await
}

/// Ejecuta `f` con un lector, fuera del hilo llamador. Un error de base dañada pide reconstruir
/// la caché.
async fn with_reader<T: Send + 'static>(
    state: &AppState,
    f: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let (readers, ingest) = (state.readers.clone(), state.ingest.clone());
    run_blocking(move || current(&readers)?.with(f).inspect_err(|e| ingest.report(e))).await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_summary", level = "info", skip_all)]
pub async fn get_summary(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
) -> CmdResult<Summary> {
    let (filter, now) = (filter.unwrap_or_default(), now_ms());
    let key = result_cache::key_at_minute("summary", &filter, now);
    cached(&state, key, move |c| queries::summary(c, &filter, now)).await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_timeseries", level = "info", skip_all)]
pub async fn get_timeseries(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
    bucket: String,
    tz: String,
) -> CmdResult<Vec<Point>> {
    let filter = filter.unwrap_or_default();
    let key = result_cache::key("timeseries", &(&filter, &bucket, &tz));
    cached(&state, key, move |c| {
        queries::timeseries(c, &filter, &bucket, &tz)
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
    let filter = filter.unwrap_or_default();
    let key = result_cache::key("timeseries_by", &(&filter, &by, &tz));
    cached(&state, key, move |c| {
        queries::timeseries_by(c, &filter, &by, &tz)
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
    let filter = filter.unwrap_or_default();
    let key = result_cache::key("breakdown", &(&filter, &by));
    cached(&state, key, move |c| queries::breakdown(c, &filter, &by)).await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_activity", level = "info", skip_all)]
pub async fn get_activity(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
) -> CmdResult<ActivityReport> {
    let filter = filter.unwrap_or_default();
    let key = result_cache::key("activity", &filter);
    cached(&state, key, move |c| insights::activity(c, &filter)).await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_activity_daily", level = "info", skip_all)]
pub async fn get_activity_daily(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
    tz: String,
) -> CmdResult<Vec<ActivityDay>> {
    let filter = filter.unwrap_or_default();
    let key = result_cache::key("activity_daily", &(&filter, &tz));
    cached(&state, key, move |c| {
        insights::activity_daily(c, &filter, &tz)
    })
    .await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_agents", level = "info", skip_all)]
pub async fn list_agents(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
) -> CmdResult<Vec<AgentRow>> {
    let filter = filter.unwrap_or_default();
    let key = result_cache::key("list_agents", &filter);
    cached(&state, key, move |c| queries::list_agents(c, &filter)).await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_projects", level = "info", skip_all)]
pub async fn list_projects(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
) -> CmdResult<Vec<ProjectRow>> {
    let filter = filter.unwrap_or_default();
    let key = result_cache::key("list_projects", &filter);
    cached(&state, key, move |c| queries::list_projects(c, &filter)).await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.get_data_info", level = "info", skip_all)]
pub async fn get_data_info(state: tauri::State<'_, AppState>) -> CmdResult<DataInfo> {
    with_reader(&state, queries::data_info).await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.export_data", level = "info", skip_all)]
pub async fn export_data(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
    format: String,
) -> CmdResult<String> {
    with_reader(&state, move |c| {
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
    with_reader(&state, move |c| {
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
    with_reader(&state, move |c| {
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
    with_reader(&state, move |c| {
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
    with_reader(&state, move |c| sessions::session_detail(c, &id)).await
}

#[tauri::command]
#[tracing::instrument(name = "cmd.list_prices", level = "info", skip_all)]
pub async fn list_prices(
    state: tauri::State<'_, AppState>,
    filter: Option<Filter>,
) -> CmdResult<Vec<crate::pricing::PriceRow>> {
    let overrides = run_blocking(|| Ok(settings::load().price_overrides)).await?;
    with_reader(&state, move |c| {
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
    let (previous, saved) = run_blocking(move || {
        let previous = settings::load();
        settings::save(&settings)?;
        Ok((previous, settings::load()))
    })
    .await?;
    // Al cambiar el presupuesto se vuelven a permitir los avisos.
    state.alerts.reset();
    // Los precios del usuario recalculan los costes al momento (sin releer logs), en el escritor.
    let overrides = saved.price_overrides.clone();
    let ingest = state.ingest.clone();
    run_blocking(move || ingest.apply_prices(overrides).map_err(anyhow::Error::msg)).await?;
    if let Some(mode) = cache_mode_change(
        previous.memory_only,
        saved.memory_only,
        crate::db::default_cache_path(),
    ) {
        let ingest = state.ingest.clone();
        run_blocking(move || ingest.rebuild(Some(mode)).map_err(anyhow::Error::msg)).await?;
    }
    Ok(saved)
}

/// Modo al que pasa la caché si «Solo memoria» cambió: a memoria (la reconstrucción borra la del
/// disco) o de vuelta al disco. `None` si no cambió.
fn cache_mode_change(
    before: bool,
    now: bool,
    path: Option<std::path::PathBuf>,
) -> Option<crate::db::CacheMode> {
    use crate::db::CacheMode;
    if before == now {
        return None;
    }
    Some(match (now, path) {
        (false, Some(path)) => CacheMode::Disco(path),
        _ => CacheMode::Memoria,
    })
}

/// Vacía la caché (en disco o en memoria) y vuelve a leer todos los logs; al terminar llega
/// `ingest://done`.
#[tauri::command]
#[tracing::instrument(name = "cmd.rebuild_cache", level = "info", skip_all)]
pub async fn rebuild_cache(state: tauri::State<'_, AppState>) -> CmdResult<()> {
    let ingest = state.ingest.clone();
    run_blocking(move || ingest.rebuild(None).map_err(anyhow::Error::msg)).await
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
    fn cambiar_solo_memoria_cambia_el_modo_de_la_cache() {
        use crate::db::CacheMode;
        let path = std::path::PathBuf::from("/tmp/x/agentboard/cache.sqlite");
        assert_eq!(cache_mode_change(false, false, Some(path.clone())), None);
        assert_eq!(cache_mode_change(true, true, Some(path.clone())), None);
        assert_eq!(
            cache_mode_change(false, true, Some(path.clone())),
            Some(CacheMode::Memoria)
        );
        assert_eq!(
            cache_mode_change(true, false, Some(path.clone())),
            Some(CacheMode::Disco(path))
        );
        assert_eq!(
            cache_mode_change(true, false, None),
            Some(CacheMode::Memoria)
        );
    }

    #[test]
    fn el_lector_vigente_es_el_del_ultimo_conjunto_publicado() {
        use std::sync::RwLock;
        let target = crate::db::Target::shared_memory();
        let writer = crate::db::open_writer(&target).unwrap();
        let readers: Readers = Arc::new(RwLock::new(Arc::new(ReadPool::open(&target, 2).unwrap())));
        let count = |readers: &Readers| {
            current(readers)
                .unwrap()
                .with(|c| {
                    Ok(c.query_row("SELECT COUNT(*) FROM agents", [], |r| r.get::<_, i64>(0))?)
                })
                .unwrap()
        };
        assert_eq!(count(&readers), 0);
        // Otro conjunto (como tras reconstruir): las consultas siguientes lo usan.
        let other = crate::db::Target::shared_memory();
        let other_writer = crate::db::open_writer(&other).unwrap();
        other_writer
            .execute("INSERT INTO agents VALUES ('a','A','/',0)", [])
            .unwrap();
        *readers.write().unwrap() = Arc::new(ReadPool::open(&other, 1).unwrap());
        assert_eq!(count(&readers), 1);
        drop(writer);
    }
}
