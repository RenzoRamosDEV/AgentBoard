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

fn scan_threads(providers: &[Box<dyn Provider>], threads: usize) -> String {
    let mut conn = db::open_in_memory().unwrap();
    let stats = ingest::scan_all_with_threads(&mut conn, providers, threads).unwrap();
    assert_eq!(stats.errors, 0, "{stats:?}");
    format!("{stats:?}\n{}", common::fingerprint(&conn).unwrap())
}

#[test]
fn uno_o_cuatro_hilos_dan_la_misma_huella() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("2m");
    if !generate_2m(&root) {
        eprintln!("python3 no disponible: se salta");
        return;
    }
    let reference = scan_threads(&common::providers(&root), 1);
    for _ in 0..5 {
        assert_eq!(scan_threads(&common::providers(&root), 4), reference);
    }
}

fn assistant_line(session: &str, i: usize) -> String {
    format!(
        r#"{{"type":"assistant","sessionId":"{session}","timestamp":"2026-09-25T10:{:02}:{:02}Z","cwd":"/h/p","message":{{"model":"claude-sonnet-4-5","id":"{session}-m{i}","content":[{{"type":"tool_use","id":"{session}-t{i}","name":"Bash","input":{{"command":"ls"}}}}],"usage":{{"input_tokens":10,"output_tokens":5}}}}}}"#,
        (i / 60) % 60,
        i % 60
    ) + "\n"
}

#[test]
fn un_archivo_grande_y_muchos_pequenos_con_dos_hilos_no_se_bloquea() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("projects");
    std::fs::create_dir_all(root.join("p")).unwrap();
    let big: String = (0..3000).map(|i| assistant_line("grande", i)).collect();
    std::fs::write(root.join("p/000-grande.jsonl"), big).unwrap();
    for f in 0..50 {
        let small: String = (0..3)
            .map(|i| assistant_line(&format!("s{f}"), i))
            .collect();
        std::fs::write(root.join(format!("p/{f:03}-pequeno.jsonl")), small).unwrap();
    }
    let make =
        || -> Vec<Box<dyn Provider>> { vec![Box::new(ClaudeCode::with_roots(vec![root.clone()]))] };
    let reference = scan_threads(&make(), 1);
    let (tx, rx) = std::sync::mpsc::channel();
    let providers = make();
    std::thread::spawn(move || tx.send(scan_threads(&providers, 2)).unwrap());
    let parallel = rx
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("el escaneo con dos hilos no terminó: interbloqueo");
    assert_eq!(parallel, reference);
}
