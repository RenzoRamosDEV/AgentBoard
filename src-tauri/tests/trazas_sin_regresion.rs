//! Las trazas no cambian lo importado ni las cifras: mismo escaneo con y sin `debug`.

use agentboard_lib::providers::{
    claude_code::ClaudeCode, codex::Codex, copilot::Copilot, cursor::Cursor, gemini::Gemini,
    Provider,
};
use agentboard_lib::{db, ingest, queries, telemetry};
use std::path::PathBuf;

fn fixtures(sub: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures")
        .join(sub)
}

fn providers() -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(ClaudeCode::with_roots(vec![fixtures("claude_code")])),
        Box::new(Codex::with_roots(vec![fixtures("codex/sessions")])),
        Box::new(Copilot::with_roots(vec![fixtures("copilot/session-state")])),
        Box::new(Cursor::with_roots(vec![fixtures("cursor/projects")])),
        Box::new(Gemini::with_roots(vec![fixtures("gemini/tmp")])),
    ]
}

fn scan_and_summarize() -> (String, String) {
    let mut conn = db::open_in_memory().expect("base en memoria");
    let stats = ingest::scan_all(&mut conn, &providers()).expect("escaneo");
    let summary = queries::summary(&conn, &queries::Filter::default(), 0).expect("resumen");
    (
        serde_json::to_string(&stats).expect("stats a JSON"),
        serde_json::to_string(&summary).expect("resumen a JSON"),
    )
}

#[test]
fn mismo_resultado_con_y_sin_trazas() {
    let without = scan_and_summarize();
    let debug =
        telemetry::subscriber_with_writer(telemetry::filter_from(Some("debug")), std::io::sink);
    let with = tracing::subscriber::with_default(debug, scan_and_summarize);
    assert_eq!(with, without);
    assert!(without.0.contains("\"errors\":0"), "{}", without.0);
}
