//! Lo que devuelve el servidor MCP de proyectos y sesiones (`list_sessions`, `project_detail`,
//! `session_detail`) no cambia al paginar los comandos de la interfaz: golden grabado en la
//! partida de la Fase 3 sobre los fixtures versionados. `UPDATE_GOLDEN=1` lo regraba.

#[path = "../benches/common/mod.rs"]
mod common;

use agentboard_lib::queries::Filter;
use agentboard_lib::{ingest, sessions};

#[test]
fn proyectos_y_sesiones_del_mcp_iguales_que_en_la_partida() {
    let dir = tempfile::tempdir().unwrap();
    let providers = common::versioned_fixture_providers(dir.path()).unwrap();
    let mut conn = common::open_measure_db().unwrap();
    ingest::scan_all(&mut conn, &providers).unwrap();
    let f = Filter::default();
    let mut out = String::new();
    let list = sessions::list_sessions(&conn, &f, None).unwrap();
    out.push_str(&common::canonical_line(
        "list_sessions",
        "todo",
        &serde_json::to_value(&list).unwrap(),
    ));
    out.push('\n');
    for p in sessions::list_projects(&conn, &f).unwrap() {
        let d = sessions::project_detail(&conn, &p.key, &f, common::TZ).unwrap();
        out.push_str(&common::canonical_line(
            "project_detail",
            &p.key,
            &serde_json::to_value(&d).unwrap(),
        ));
        out.push('\n');
    }
    for s in &list.sessions {
        let d = sessions::session_detail(&conn, &s.id).unwrap();
        out.push_str(&common::canonical_line(
            "session_detail",
            &s.id,
            &serde_json::to_value(&d).unwrap(),
        ));
        out.push('\n');
    }
    let path = common::manifest_dir().join("tests/golden/detalle_mcp_fixtures.txt");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &out).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).expect("falta el golden: UPDATE_GOLDEN=1");
    if let Err(diff) = common::compare_golden(&expected, &out) {
        panic!("la salida del MCP cambió:\n{diff}");
    }
}
