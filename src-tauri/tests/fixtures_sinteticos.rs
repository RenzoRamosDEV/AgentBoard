//! Los logs de `scripts/gen-fixtures.py` se importan sin errores y con llamadas de los cinco agentes.

use agentboard_lib::providers::{
    claude_code::ClaudeCode, codex::Codex, copilot::Copilot, cursor::Cursor, gemini::Gemini,
    Provider,
};
use agentboard_lib::{db, ingest};
use std::path::{Path, PathBuf};
use std::process::Command;

fn generate(out: &Path) -> bool {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/gen-fixtures.py");
    match Command::new("python3")
        .arg(script)
        .args(["2m", "--seed", "11", "--out"])
        .arg(out)
        .output()
    {
        Ok(o) => {
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            true
        }
        Err(_) => false,
    }
}

#[test]
fn fixture_sintetico_se_importa_sin_errores_con_los_cinco_agentes() {
    let dir = tempfile::tempdir().expect("carpeta temporal");
    let root = dir.path().join("2m");
    if !generate(&root) {
        eprintln!("python3 no disponible: se salta el test de fixtures sintéticos");
        return;
    }
    let providers: Vec<Box<dyn Provider>> = vec![
        Box::new(ClaudeCode::with_roots(vec![root.join("claude_code")])),
        Box::new(Codex::with_roots(vec![root.join("codex/sessions")])),
        Box::new(Copilot::with_roots(
            vec![root.join("copilot/session-state")],
        )),
        Box::new(Cursor::with_roots(vec![root.join("cursor/projects")])),
        Box::new(Gemini::with_roots(vec![root.join("gemini/tmp")])),
    ];
    let mut conn = db::open_in_memory().expect("base");
    let stats = ingest::scan_all(&mut conn, &providers).expect("escaneo");
    assert_eq!(stats.errors, 0, "{stats:?}");

    for agent in ["claude-code", "codex", "copilot", "cursor", "gemini"] {
        let calls: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM calls c JOIN sessions s ON s.id = c.session_id WHERE s.agent_id = ?1",
                [agent],
                |r| r.get(0),
            )
            .expect("cuenta de llamadas");
        assert!(calls > 0, "{agent} sin llamadas");
    }
    let tool_calls: i64 = conn
        .query_row("SELECT COUNT(*) FROM tool_calls", [], |r| r.get(0))
        .expect("tool calls");
    assert!(tool_calls > 0, "sin usos de herramientas");
}
