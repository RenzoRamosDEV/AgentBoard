//! Equivalencia de la ingesta: la huella de los datos importados de los fixtures sintéticos y
//! versionados coincide con los golden grabados antes de optimizar la ingesta.
//! `UPDATE_GOLDEN=1 cargo test --test equivalencia_ingesta` los regraba.

#[path = "../benches/common/mod.rs"]
mod common;

use agentboard_lib::providers::{
    claude_code::ClaudeCode, codex::Codex, copilot::Copilot, cursor::Cursor, gemini::Gemini,
    opencode::OpenCode, Provider,
};
use agentboard_lib::{db, ingest};
use std::path::{Path, PathBuf};
use std::process::Command;

fn golden(name: &str) -> PathBuf {
    common::manifest_dir().join("tests/golden").join(name)
}

fn check_golden(name: &str, actual: &str) {
    let path = golden(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "falta {}: grábalo con UPDATE_GOLDEN=1 en el código de referencia",
            path.display()
        )
    });
    assert_eq!(actual, expected, "la huella de {name} cambió");
}

fn scan(providers: &[Box<dyn Provider>]) -> String {
    let mut conn = db::open_in_memory().unwrap();
    let stats = ingest::scan_all(&mut conn, providers).unwrap();
    assert_eq!(stats.errors, 0, "{stats:?}");
    format!(
        "archivos {}, líneas {}, registros {}\n{}",
        stats.files,
        stats.lines,
        stats.records,
        common::fingerprint(&conn).unwrap()
    )
}

fn generate_2m(out: &Path) -> bool {
    let script = common::manifest_dir().join("../scripts/gen-fixtures.py");
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
fn fixture_sintetico_2m_igual_que_el_golden() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("2m");
    if !generate_2m(&root) {
        eprintln!("python3 no disponible: se salta la equivalencia del fixture sintético");
        return;
    }
    check_golden("ingesta_2m.txt", &scan(&common::providers(&root)));
}

#[test]
fn fixtures_versionados_iguales_que_el_golden() {
    let fx = common::manifest_dir().join("../tests/fixtures");
    let dir = tempfile::tempdir().unwrap();
    let oc_root = dir.path().join("opencode");
    std::fs::create_dir_all(&oc_root).unwrap();
    let sql = std::fs::read_to_string(fx.join("opencode/opencode.sql")).unwrap();
    rusqlite::Connection::open(oc_root.join("opencode.db"))
        .unwrap()
        .execute_batch(&sql)
        .unwrap();
    let providers: Vec<Box<dyn Provider>> = vec![
        Box::new(ClaudeCode::with_roots(vec![fx.join("claude_code")])),
        Box::new(Codex::with_roots(vec![fx.join("codex/sessions")])),
        Box::new(Copilot::with_roots(vec![fx.join("copilot/session-state")])),
        Box::new(Cursor::with_roots(vec![fx.join("cursor/projects")])),
        Box::new(Gemini::with_roots(vec![fx.join("gemini/tmp")])),
        Box::new(OpenCode::with_roots(vec![oc_root])),
    ];
    check_golden("ingesta_fixtures.txt", &scan(&providers));
}
