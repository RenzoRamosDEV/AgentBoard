//! El Resumen en una sola petición: todos sus apartados (y el periodo anterior de la
//! comparación) en una transacción de lectura, para que vean el mismo estado de los datos, con
//! `turn_stats` calculado una vez y compartido por `activity` y `activity_daily`.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use crate::ingest_thread::DataGeneration;
use crate::insights::{self, ActivityDay, ActivityReport};
use crate::queries::{self, BreakdownRow, Filter, Point, SeriesPoint, Summary};
use crate::result_cache::{key_at_minute, ResultCache};
use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Lo que pide el Resumen: el filtro, la zona de la interfaz, el inicio del mes en curso (lo
/// calcula la interfaz en su hora local), si las series por proyecto van por rama (un solo
/// proyecto elegido) y el periodo anterior de la comparación.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardRequest {
    pub filter: Filter,
    pub tz: String,
    pub month_from: i64,
    #[serde(default)]
    pub branch_view: bool,
    #[serde(default)]
    pub previous: Option<Filter>,
}

/// Periodo anterior de la comparación.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviousSnapshot {
    pub summary: Summary,
    pub daily: Vec<Point>,
    pub projects: Vec<BreakdownRow>,
}

/// Los apartados del Resumen, con los mismos tipos que sus comandos sueltos.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub summary: Summary,
    pub agents: Vec<BreakdownRow>,
    pub daily: Vec<Point>,
    pub hourly: Vec<Point>,
    pub daily_by_agent: Vec<SeriesPoint>,
    pub daily_by_model: Vec<SeriesPoint>,
    /// Por proyecto, o por rama con `branch_view`.
    pub daily_by_project: Vec<SeriesPoint>,
    pub daily_by_tool: Vec<SeriesPoint>,
    pub month: Vec<Point>,
    pub projects: Vec<BreakdownRow>,
    /// Solo con `branch_view`.
    pub branches: Option<Vec<BreakdownRow>>,
    pub models: Vec<BreakdownRow>,
    pub activity: ActivityReport,
    pub activity_daily: Vec<ActivityDay>,
    pub tools: Vec<BreakdownRow>,
    pub commands: Vec<BreakdownRow>,
    pub skills: Vec<BreakdownRow>,
    pub mcp: Vec<BreakdownRow>,
    pub agent_types: Vec<BreakdownRow>,
    pub prev: Option<PreviousSnapshot>,
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn dashboard(conn: &Connection, req: &DashboardRequest, now: i64) -> Result<DashboardSnapshot> {
    dashboard_with(conn, req, now, &mut || {})
}

/// `dashboard` llamando a `between` entre apartados (los tests confirman lotes ahí).
fn dashboard_with(
    conn: &Connection,
    req: &DashboardRequest,
    now: i64,
    between: &mut dyn FnMut(),
) -> Result<DashboardSnapshot> {
    // Una transacción de lectura: todos los apartados ven el mismo estado confirmado.
    let tx = conn.unchecked_transaction()?;
    let (f, tz) = (&req.filter, req.tz.as_str());
    let project_key = if req.branch_view { "branch" } else { "project" };
    let month = Filter {
        from: Some(req.month_from),
        to: None,
        ..f.clone()
    };
    let summary = queries::summary(&tx, f, now)?;
    between();
    let agents = queries::breakdown(&tx, f, "agent")?;
    between();
    let daily = queries::timeseries(&tx, f, "day", tz)?;
    let hourly = queries::timeseries(&tx, f, "hour", tz)?;
    between();
    let daily_by_agent = queries::timeseries_by(&tx, f, "agent", tz)?;
    let daily_by_model = queries::timeseries_by(&tx, f, "model", tz)?;
    let daily_by_project = queries::timeseries_by(&tx, f, project_key, tz)?;
    let daily_by_tool = queries::timeseries_by(&tx, f, "tool", tz)?;
    between();
    let month = queries::timeseries(&tx, &month, "day", tz)?;
    let projects = queries::breakdown(&tx, f, "project")?;
    let branches = if req.branch_view {
        Some(queries::breakdown(&tx, f, "branch")?)
    } else {
        None
    };
    let models = queries::breakdown(&tx, f, "model")?;
    between();
    let turns = insights::turn_stats(&tx, f)?;
    let activity = insights::activity_from(&tx, f, &turns)?;
    let activity_daily = insights::activity_daily_from(&tx, f, tz, &turns)?;
    drop(turns);
    between();
    let tools = queries::breakdown(&tx, f, "tool")?;
    let commands = queries::breakdown(&tx, f, "command")?;
    let skills = queries::breakdown(&tx, f, "skill")?;
    let mcp = queries::breakdown(&tx, f, "mcp")?;
    let agent_types = queries::breakdown(&tx, f, "agent_type")?;
    let prev = match &req.previous {
        Some(p) => Some(PreviousSnapshot {
            summary: queries::summary(&tx, p, now)?,
            daily: queries::timeseries(&tx, p, "day", tz)?,
            projects: queries::breakdown(&tx, p, project_key)?,
        }),
        None => None,
    };
    tx.commit()?;
    Ok(DashboardSnapshot {
        summary,
        agents,
        daily,
        hourly,
        daily_by_agent,
        daily_by_model,
        daily_by_project,
        daily_by_tool,
        month,
        projects,
        branches,
        models,
        activity,
        activity_daily,
        tools,
        commands,
        skills,
        mcp,
        agent_types,
        prev,
    })
}

/// El snapshot a través de la caché de resultados: la cabecera depende del minuto (ritmo de la
/// última hora), así que la clave también.
pub fn cached(
    cache: &ResultCache,
    generation: &DataGeneration,
    req: &DashboardRequest,
    now: i64,
    compute: impl FnOnce() -> Result<DashboardSnapshot>,
) -> Result<Arc<DashboardSnapshot>> {
    cache.get_or_compute(generation, key_at_minute("snapshot", req, now), compute)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn call(conn: &Connection, id: &str, ts: i64) {
        conn.execute(
            "INSERT INTO calls (message_id, session_id, ts, model, input_tokens, output_tokens)
             VALUES (?1, 's', ?2, 'claude-sonnet-4-5', 1000, 10)",
            rusqlite::params![id, ts],
        )
        .unwrap();
        crate::derived::refresh(conn).unwrap();
    }

    fn request() -> DashboardRequest {
        DashboardRequest {
            filter: Filter::default(),
            tz: "UTC".into(),
            month_from: 0,
            branch_view: false,
            previous: None,
        }
    }

    fn on_disk() -> (tempfile::TempDir, db::Storage) {
        let dir = tempfile::tempdir().unwrap();
        let storage = db::Storage::open(db::CacheMode::Disco(dir.path().join("c.sqlite"))).unwrap();
        storage
            .writer
            .execute_batch(
                "INSERT INTO agents VALUES ('a','A','',0);
                 INSERT INTO sessions (id, agent_id, started_at, ended_at) VALUES ('s','a',1000,1000);
                 INSERT INTO turns (id, session_id, ts, intent) VALUES ('t','s',1000,NULL);",
            )
            .unwrap();
        call(&storage.writer, "m1", 1000);
        (dir, storage)
    }

    #[test]
    fn todos_los_apartados_ven_el_mismo_estado_aunque_se_confirme_un_lote_entre_medias() {
        let (_dir, storage) = on_disk();
        let reader = db::ReadPool::open(&storage.target, 1).unwrap();
        let writer = std::sync::Mutex::new(storage.writer);
        let mut n = 1;
        let mut between = || {
            n += 1;
            call(&writer.lock().unwrap(), &format!("m{n}"), 1000 + n);
        };
        let snap = reader
            .with(|c| dashboard_with(c, &request(), 5000, &mut between))
            .unwrap();
        assert!(n > 2, "el gancho corrió entre apartados");
        let agents: i64 = snap.agents.iter().map(|r| r.calls).sum();
        assert_eq!(snap.summary.calls, 1, "el estado del principio");
        assert_eq!(agents, 1);
        assert_eq!(snap.daily.iter().map(|p| p.calls).sum::<i64>(), 1);
    }

    #[test]
    fn el_gasto_del_mes_va_desde_su_inicio_sin_el_final_del_periodo() {
        let (_dir, storage) = on_disk();
        call(&storage.writer, "m2", 3000);
        let req = DashboardRequest {
            filter: Filter {
                from: Some(0),
                to: Some(2000),
                ..Default::default()
            },
            month_from: 2500,
            ..request()
        };
        let snap = dashboard(&storage.writer, &req, 5000).unwrap();
        assert_eq!(
            snap.month.iter().map(|p| p.calls).sum::<i64>(),
            1,
            "solo m2"
        );
        assert_eq!(snap.summary.calls, 1, "solo m1");
    }

    #[test]
    fn turn_stats_se_calcula_una_vez_por_snapshot() {
        let (_dir, storage) = on_disk();
        let before = crate::insights::turn_stats_calls();
        dashboard(&storage.writer, &request(), 5000).unwrap();
        assert_eq!(crate::insights::turn_stats_calls() - before, 1);
        let mut req = request();
        req.previous = Some(Filter::default());
        req.branch_view = true;
        let before = crate::insights::turn_stats_calls();
        dashboard(&storage.writer, &req, 5000).unwrap();
        assert_eq!(crate::insights::turn_stats_calls() - before, 1);
    }

    #[test]
    fn dos_peticiones_iguales_en_el_mismo_minuto_salen_de_la_cache() {
        let (_dir, storage) = on_disk();
        let cache = crate::result_cache::ResultCache::default();
        let generation = crate::ingest_thread::DataGeneration::default();
        let now = 60_000 * 1000;
        let before = crate::insights::turn_stats_calls();
        let a = cached(&cache, &generation, &request(), now, || {
            dashboard(&storage.writer, &request(), now)
        })
        .unwrap();
        let b = cached(&cache, &generation, &request(), now + 59_000, || {
            dashboard(&storage.writer, &request(), now + 59_000)
        })
        .unwrap();
        assert!(std::sync::Arc::ptr_eq(&a, &b));
        assert_eq!(crate::insights::turn_stats_calls() - before, 1);
        // Otro minuto u otra generación: se recalcula.
        cached(&cache, &generation, &request(), now + 60_000, || {
            dashboard(&storage.writer, &request(), now + 60_000)
        })
        .unwrap();
        generation.bump_for_tests();
        cached(&cache, &generation, &request(), now + 60_000, || {
            dashboard(&storage.writer, &request(), now + 60_000)
        })
        .unwrap();
        assert_eq!(crate::insights::turn_stats_calls() - before, 3);
    }
}
