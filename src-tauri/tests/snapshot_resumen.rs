//! El Resumen en una sola petición (`snapshot::dashboard`) devuelve en cada apartado lo mismo que
//! su consulta suelta, con el comparador del golden de consultas.

#[path = "../benches/common/mod.rs"]
mod common;

use agentboard_lib::queries::{self, Filter};
use agentboard_lib::snapshot::{self, DashboardRequest};
use agentboard_lib::{db, ingest, insights};
use rusqlite::Connection;
use serde_json::{json, Value};

const DAY: i64 = common::DAY_MS;

fn v<T: serde::Serialize>(x: T) -> Value {
    serde_json::to_value(x).unwrap()
}

/// Lo que devolvían los comandos sueltos que pedía `useDashboardData`.
fn separately(conn: &Connection, req: &DashboardRequest, now: i64) -> Value {
    let (f, tz) = (&req.filter, req.tz.as_str());
    let by = |k: &str| v(queries::breakdown(conn, f, k).unwrap());
    let series_by = |k: &str| v(queries::timeseries_by(conn, f, k, tz).unwrap());
    let month = Filter {
        from: Some(req.month_from),
        to: None,
        ..f.clone()
    };
    let project_key = if req.branch_view { "branch" } else { "project" };
    let prev = req.previous.as_ref().map(|p| {
        json!({
            "summary": v(queries::summary(conn, p, now).unwrap()),
            "daily": v(queries::timeseries(conn, p, "day", tz).unwrap()),
            "projects": v(queries::breakdown(conn, p, project_key).unwrap()),
        })
    });
    json!({
        "summary": v(queries::summary(conn, f, now).unwrap()),
        "agents": by("agent"),
        "daily": v(queries::timeseries(conn, f, "day", tz).unwrap()),
        "hourly": v(queries::timeseries(conn, f, "hour", tz).unwrap()),
        "dailyByAgent": series_by("agent"),
        "dailyByModel": series_by("model"),
        "dailyByProject": series_by(project_key),
        "dailyByTool": series_by("tool"),
        "month": v(queries::timeseries(conn, &month, "day", tz).unwrap()),
        "projects": by("project"),
        "branches": if req.branch_view { by("branch") } else { Value::Null },
        "models": by("model"),
        "activity": v(insights::activity(conn, f).unwrap()),
        "activityDaily": v(insights::activity_daily(conn, f, tz).unwrap()),
        "tools": by("tool"),
        "commands": by("command"),
        "skills": by("skill"),
        "mcp": by("mcp"),
        "agentTypes": by("agent_type"),
        "prev": prev,
    })
}

fn check_all(conn: &Connection, what: &str) {
    let (now, filters) = common::golden_filters(conn).unwrap();
    let mut checked = 0;
    for (name, f) in filters {
        let previous = f.from.map(|from| Filter {
            from: Some(from - 30 * DAY),
            to: Some(from),
            ..f.clone()
        });
        for branch_view in [false, true] {
            for previous in [None, previous.clone()] {
                let req = DashboardRequest {
                    filter: f.clone(),
                    tz: common::TZ.to_string(),
                    month_from: common::MONTH_START_MS,
                    branch_view,
                    previous,
                };
                let got = v(snapshot::dashboard(conn, &req, now).unwrap());
                let expected = separately(conn, &req, now);
                if let Err(diff) = common::compare_json(&expected, &got) {
                    panic!("{what}, filtro {name}, ramas {branch_view}: {diff}");
                }
                checked += 1;
            }
        }
    }
    assert!(checked >= 8, "{checked}");
}

fn scanned(providers: &[Box<dyn agentboard_lib::providers::Provider>]) -> Connection {
    let mut conn = common::open_measure_db().unwrap();
    let stats = ingest::scan_all(&mut conn, providers).unwrap();
    assert_eq!(stats.errors, 0);
    conn
}

#[test]
fn cada_apartado_es_igual_a_su_consulta_suelta_en_el_fixture_2m() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("2m");
    if !common::generate_fixture("2m", 11, &root) {
        eprintln!("python3 no disponible: se salta");
        return;
    }
    check_all(&scanned(&common::providers(&root)), "2m");
}

#[test]
fn cada_apartado_es_igual_a_su_consulta_suelta_en_los_fixtures_versionados() {
    let dir = tempfile::tempdir().unwrap();
    let providers = common::versioned_fixture_providers(dir.path()).unwrap();
    check_all(&scanned(&providers), "tests/fixtures");
}

#[test]
fn tambien_en_una_cache_en_disco() {
    let dir = tempfile::tempdir().unwrap();
    let storage = db::Storage::open(db::CacheMode::Disco(dir.path().join("c.sqlite"))).unwrap();
    let mut writer = storage.writer;
    db::set_cache_tz(&writer, common::TZ).unwrap();
    let providers = common::versioned_fixture_providers(dir.path()).unwrap();
    ingest::scan_all(&mut writer, &providers).unwrap();
    let reader = db::ReadPool::open(&storage.target, 1).unwrap();
    reader
        .with(|c| {
            check_all(c, "disco");
            Ok(())
        })
        .unwrap();
}
