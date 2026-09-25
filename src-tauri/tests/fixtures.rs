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
    assert_eq!(stats.files, 1);
    assert_eq!(stats.errors, 0);

    let s = queries::summary(&conn, &queries::Filter::default()).unwrap();
    assert_eq!(s.calls, 2, "msg_demo_1 aparece dos veces por streaming");
    assert_eq!(s.sessions, 1);
    assert_eq!(s.output_tokens, 320);
    assert!(s.unpriced_models.is_empty());
    // Opus 5.5: 5 in × 4 + 320 out × 20 + 12000 read × 0.2 + 12500 write1h × 8, por millón
    let expected = (5.0 * 4.0 + 320.0 * 20.0 + 12000.0 * 0.2 + 12500.0 * 8.0) / 1e6;
    assert!((s.cost_usd - expected).abs() < 1e-9, "{} != {expected}", s.cost_usd);

    let q = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(q("SELECT COUNT(*) FROM tool_calls"), 2);
    assert_eq!(q("SELECT is_error FROM tool_calls WHERE call_id='toolu_demo_1'"), 1);
    assert_eq!(q("SELECT duration_ms FROM tool_calls WHERE call_id='toolu_demo_1'"), 6000);
    assert_eq!(q("SELECT COUNT(*) FROM events WHERE kind='compaction'"), 1);
    assert_eq!(q("SELECT COUNT(*) FROM events WHERE kind='interruption'"), 1);
    let (name, branch): (String, String) = conn
        .query_row("SELECT p.name, s.git_branch FROM sessions s JOIN projects p ON p.id = s.project_id", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((name.as_str(), branch.as_str()), ("demo", "main"));
    let activity: String = conn
        .query_row("SELECT activity FROM calls WHERE message_id='msg_demo_1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(activity, "testing");
}
