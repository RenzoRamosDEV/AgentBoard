//! Importa los fixtures anonimizados de `tests/fixtures/` y comprueba los totales.

use agentburn_lib::providers::claude_code::ClaudeCode;
use agentburn_lib::providers::Provider;
use agentburn_lib::{db, ingest, queries};
use std::path::PathBuf;

fn fixtures(agent: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures").join(agent)
}

#[test]
fn claude_code_fixture() {
    let mut conn = db::open_in_memory().unwrap();
    let providers: Vec<Box<dyn Provider>> = vec![Box::new(ClaudeCode::with_roots(vec![fixtures("claude_code")]))];
    let stats = ingest::scan_all(&mut conn, &providers).unwrap();
    assert_eq!(stats.files, 2, "sesión principal + subagente");
    assert_eq!(stats.errors, 0);

    let s = queries::summary(&conn, &queries::Filter::default(), 0).unwrap();
    assert_eq!(s.calls, 8, "msg_demo_1 aparece dos veces por streaming");
    assert_eq!(s.sessions, 1);
    assert!(s.unpriced_models.is_empty());

    fn count(conn: &rusqlite::Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }
    let q = |sql: &str| count(&conn, sql);
    assert_eq!(q("SELECT COUNT(*) FROM turns"), 2);
    assert_eq!(q("SELECT COUNT(*) FROM calls WHERE turn_id IS NULL"), 0);
    assert_eq!(q("SELECT COUNT(*) FROM calls WHERE turn_id = 'p2'"), 5, "3 principales + 2 del subagente");
    assert_eq!(q("SELECT COUNT(*) FROM calls WHERE is_sidechain = 1 AND agent_id = 'a1b2c3'"), 2);
    assert_eq!(q("SELECT is_error FROM tool_calls WHERE call_id='toolu_demo_1'"), 1);
    assert_eq!(q("SELECT duration_ms FROM tool_calls WHERE call_id='toolu_demo_1'"), 6000);
    assert_eq!(q("SELECT COUNT(*) FROM tool_calls WHERE agent_id = 'a1b2c3' AND detail = 'Explore'"), 1);
    assert_eq!(q("SELECT COUNT(*) FROM events WHERE kind='compaction'"), 1);
    assert_eq!(q("SELECT COUNT(*) FROM events WHERE kind='interruption'"), 1);
    let intent: String = conn.query_row("SELECT intent FROM turns WHERE id='p1'", [], |r| r.get(0)).unwrap();
    assert_eq!(intent, "debug");
    let (name, branch): (String, String) = conn
        .query_row("SELECT p.name, s.git_branch FROM sessions s JOIN projects p ON p.id = s.project_id", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((name.as_str(), branch.as_str()), ("demo", "main"));
    let model: String = conn.query_row("SELECT model FROM sessions", [], |r| r.get(0)).unwrap();
    assert_eq!(model, "claude-opus-5-5", "las llamadas del subagente no cambian el modelo de la sesión");

    // Releer tras "actualizar la app" (file_state vacío) no duplica.
    conn.execute("DELETE FROM file_state", []).unwrap();
    ingest::scan_all(&mut conn, &providers).unwrap();
    assert_eq!(queries::summary(&conn, &queries::Filter::default(), 0).unwrap().calls, 8);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM tool_calls"), 7);
}
