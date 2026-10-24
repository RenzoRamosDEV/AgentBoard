//! Piezas puras de `examples/perf_scan.rs` (módulo común de `benches/`).

#[path = "../benches/common/mod.rs"]
mod common;

use common::{live_lines, parse_args, vm_hwm_kib, Args, LiveFile, Phase};
use std::path::PathBuf;

fn args(list: &[&str]) -> Result<Args, String> {
    parse_args(list.iter().map(|s| s.to_string()))
}

#[test]
fn sin_fixtures_es_error() {
    assert!(args(&[]).unwrap_err().contains("--fixtures"));
    assert!(args(&["--phase", "cold"]).is_err());
}

#[test]
fn valores_por_defecto() {
    assert_eq!(
        args(&["--fixtures", "fx"]).unwrap(),
        Args {
            fixtures: PathBuf::from("fx"),
            phase: Phase::All,
            live_lines: 1000,
            repeat: 1
        }
    );
}

#[test]
fn todas_las_opciones() {
    let a = args(&[
        "--phase",
        "live",
        "--repeat",
        "200",
        "--fixtures",
        "d",
        "--live-lines",
        "1",
    ])
    .unwrap();
    assert_eq!((a.phase, a.repeat, a.live_lines), (Phase::Live, 200, 1));
    for (raw, phase) in [
        ("cold", Phase::Cold),
        ("queries", Phase::Queries),
        ("all", Phase::All),
    ] {
        assert_eq!(
            args(&["--fixtures", "d", "--phase", raw]).unwrap().phase,
            phase
        );
    }
}

#[test]
fn valores_invalidos() {
    for bad in [
        &["--fixtures", "d", "--phase", "tibia"][..],
        &["--fixtures", "d", "--repeat", "0"],
        &["--fixtures", "d", "--live-lines", "-1"],
        &["--fixtures", "d", "--live-lines", "mil"],
        &["--fixtures"],
        &["--fixtures", "d", "--otra"],
    ] {
        assert!(args(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn memoria_pico_de_proc_status() {
    let status =
        "Name:\tperf_scan\nVmPeak:\t  999999 kB\nVmHWM:\t   123456 kB\nVmRSS:\t   1000 kB\n";
    assert_eq!(vm_hwm_kib(status), Some(123_456));
    assert_eq!(vm_hwm_kib("VmRSS:\t 1 kB\n"), None);
    assert_eq!(vm_hwm_kib("VmHWM:\t muchos kB\n"), None);
    assert_eq!(vm_hwm_kib(""), None);
}

#[test]
fn el_archivo_en_vivo_se_borra_al_soltarse() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("perf-live-1.jsonl");
    std::fs::write(&path, "x\n").unwrap();
    {
        let _guard = LiveFile::new(path.clone());
        assert!(path.exists());
    }
    assert!(!path.exists());
    drop(LiveFile::new(dir.path().join("nunca-creado.jsonl")));
}

#[test]
fn el_archivo_en_vivo_se_borra_aunque_la_medicion_falle() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("perf-live-2.jsonl");
    std::fs::write(&path, "x\n").unwrap();
    let p = path.clone();
    let result = std::panic::catch_unwind(move || {
        let _guard = LiveFile::new(p);
        panic!("fallo simulado");
    });
    assert!(result.is_err());
    assert!(!path.exists());
}

#[test]
fn lineas_en_vivo_json_con_ids_unicos() {
    let a = live_lines("s", 0, 3);
    let b = live_lines("s", 3, 2);
    assert_eq!(a.lines().count(), 3);
    assert!(a.ends_with('\n'));
    let ids: std::collections::HashSet<String> = a
        .lines()
        .chain(b.lines())
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            assert_eq!(v["type"], "assistant");
            v["message"]["id"].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(ids.len(), 5);
}

// ------------------------------------------------------------------ huella (D1)

use common::fingerprint;
use rusqlite::Connection;

const AGENTES: &[&str] = &["INSERT INTO agents (id, name, log_root, first_seen) VALUES ('claude-code', 'Claude Code', '/r', 1)"];
const PROYECTOS: &[&str] = &[
    "INSERT INTO projects (name, cwd, repo_root) VALUES ('a', '/h/a', '/h/a')",
    "INSERT INTO projects (name, cwd, repo_root) VALUES ('b', '/h/b', '/h/b')",
];
const SESIONES: &[&str] = &[
    "INSERT INTO sessions (id, agent_id, project_id, git_branch, started_at, ended_at, model, is_subagent)
     VALUES ('s1', 'claude-code', (SELECT id FROM projects WHERE cwd = '/h/a'), 'main', 10, 50, 'claude-sonnet-4-5', 0)",
    "INSERT INTO sessions (id, agent_id, project_id, git_branch, started_at, ended_at, model, is_subagent)
     VALUES ('s2', 'claude-code', (SELECT id FROM projects WHERE cwd = '/h/b'), NULL, 20, 60, NULL, 1)",
];
const RESTO: &[&str] = &[
    "INSERT INTO turns (id, session_id, ts, intent) VALUES ('t1', 's1', 10, 'debug')",
    "INSERT INTO calls (message_id, session_id, ts, model, input_tokens, output_tokens, cache_read, cache_write, turn_id)
     VALUES ('m1', 's1', 11, 'claude-sonnet-4-5', 100, 20, 1000, 50, 't1')",
    "INSERT INTO calls (message_id, session_id, ts, model, input_tokens, output_tokens, turn_id)
     VALUES ('m2', 's2', 21, 'claude-sonnet-4-5', 7, 3, NULL)",
    "INSERT INTO tool_calls (call_id, message_id, session_id, ts, tool, target, turn_id) VALUES ('c1', 'm1', 's1', 11, 'Bash', 'ls', 't1')",
    "INSERT INTO tool_calls (call_id, message_id, session_id, ts, tool, target, turn_id) VALUES ('c2', 'm2', 's2', 21, 'Read', NULL, NULL)",
    "INSERT INTO events (session_id, ts, kind, payload_json) VALUES ('s1', 12, 'compaction', NULL)",
    "INSERT INTO events (session_id, ts, kind, payload_json) VALUES ('s2', 22, 'interruption', '{}')",
    "INSERT INTO file_state (path, agent_id, file_id, size, offset, mtime, last_scan) VALUES ('/f', 'claude-code', '1:2', 10, 10, 5, 99)",
];

fn base(reverse: bool, extra: &[&str]) -> Connection {
    let conn = agentboard_lib::db::open_in_memory().unwrap();
    for group in [AGENTES, PROYECTOS, SESIONES, RESTO] {
        let mut sql: Vec<&str> = group.to_vec();
        if reverse {
            sql.reverse();
        }
        for s in sql {
            conn.execute(s, []).unwrap();
        }
    }
    for s in extra {
        conn.execute(s, []).unwrap();
    }
    conn
}

fn huella(conn: &Connection) -> String {
    fingerprint(conn).unwrap()
}

#[test]
fn la_huella_no_depende_del_orden_de_insercion() {
    let a = huella(&base(false, &[]));
    assert_eq!(a, huella(&base(true, &[])));
    assert!(a.contains("claude-code"), "{a}");
}

#[test]
fn la_huella_cambia_con_tokens_turno_o_cwd() {
    let original = huella(&base(false, &[]));
    for cambio in [
        "UPDATE calls SET output_tokens = 21 WHERE message_id = 'm1'",
        "UPDATE calls SET turn_id = 't1' WHERE message_id = 'm2'",
        "UPDATE tool_calls SET turn_id = NULL WHERE call_id = 'c1'",
        "UPDATE projects SET cwd = '/h/z' WHERE cwd = '/h/b'",
        "UPDATE events SET kind = 'rate_limit' WHERE session_id = 's2'",
        "UPDATE sessions SET ended_at = 61 WHERE id = 's2'",
        "UPDATE turns SET intent = NULL WHERE id = 't1'",
    ] {
        assert_ne!(huella(&base(false, &[cambio])), original, "{cambio}");
    }
}

#[test]
fn ids_autoincrementales_y_marcas_de_reloj_no_influyen() {
    let original = huella(&base(false, &[]));
    for cambio in [
        "UPDATE agents SET first_seen = 12345",
        "UPDATE file_state SET last_scan = 777",
        "UPDATE tool_calls SET id = id + 100",
        "UPDATE events SET id = id + 100",
    ] {
        assert_eq!(huella(&base(false, &[cambio])), original, "{cambio}");
    }
}
