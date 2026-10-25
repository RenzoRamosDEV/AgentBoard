//! Piezas puras de `examples/perf_scan.rs` (módulo común de `benches/`).

#[path = "../benches/common/mod.rs"]
mod common;

use common::{
    cpu_ms, live_lines, min_gap, parse_args, refresh_segments, vm_hwm_kib, Args, LiveFile, Phase,
    Reader,
};
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
            repeat: 1,
            fingerprint: false,
            query_fingerprint: false,
            cache: None,
            events: None,
            idle_secs: 60,
            reader: Reader::Loop,
        }
    );
}

#[test]
fn opciones_de_la_fase_3() {
    for (raw, phase) in [("watch", Phase::Watch), ("idle", Phase::Idle)] {
        assert_eq!(
            args(&["--fixtures", "d", "--phase", raw]).unwrap().phase,
            phase
        );
    }
    let a = args(&[
        "--fixtures",
        "d",
        "--events",
        "500",
        "--idle-secs",
        "5",
        "--reader",
        "ui",
    ])
    .unwrap();
    assert_eq!(
        (a.events, a.idle_secs, a.reader),
        (Some(500), 5, Reader::Ui)
    );
    assert_eq!(
        args(&["--fixtures", "d", "--reader", "loop"])
            .unwrap()
            .reader,
        Reader::Loop
    );
    for bad in [
        &["--fixtures", "d", "--reader", "otro"][..],
        &["--fixtures", "d", "--reader"],
        &["--fixtures", "d", "--events", "0"],
        &["--fixtures", "d", "--events", "x"],
        &["--fixtures", "d", "--idle-secs", "0"],
        &["--fixtures", "d", "--idle-secs", "-3"],
    ] {
        assert!(args(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn separacion_minima_entre_avisos() {
    use std::time::{Duration, Instant};
    let t0 = Instant::now();
    let at = |ms: u64| t0 + Duration::from_millis(ms);
    assert_eq!(min_gap(&[]), None);
    assert_eq!(min_gap(&[at(0)]), None);
    assert_eq!(
        min_gap(&[at(0), at(1000)]),
        Some(Duration::from_millis(1000))
    );
    assert_eq!(
        min_gap(&[at(0), at(1500), at(2499), at(4000)]),
        Some(Duration::from_millis(999))
    );
    assert_eq!(min_gap(&[at(5), at(5)]), Some(Duration::ZERO));
}

#[test]
fn cpu_de_proc_self_stat() {
    // Campos 14 y 15 (utime, stime) en ticks de 10 ms; el nombre puede tener espacios y paréntesis.
    let stat = |u: u64, s: u64| {
        format!("123 (perf (scan) x) S 1 2 3 4 5 6 7 8 9 10 {u} {s} 0 0 20 0 1 0 100 0 0")
    };
    assert_eq!(cpu_ms(&stat(10, 5), &stat(10, 5)), Some(0));
    assert_eq!(cpu_ms(&stat(10, 5), &stat(11, 5)), Some(10));
    assert_eq!(cpu_ms(&stat(10, 5), &stat(13, 9)), Some(70));
    assert_eq!(cpu_ms(&stat(10, 5), &stat(9, 5)), Some(0));
    assert_eq!(cpu_ms("", &stat(1, 1)), None);
    assert_eq!(cpu_ms(&stat(1, 1), "1 (x) S 2"), None);
}

#[test]
fn opciones_de_la_fase_2() {
    let a = args(&[
        "--fixtures",
        "d",
        "--query-fingerprint",
        "--cache",
        "/tmp/c.sqlite",
    ])
    .unwrap();
    assert!(a.query_fingerprint);
    assert!(!a.fingerprint);
    assert_eq!(a.cache, Some(PathBuf::from("/tmp/c.sqlite")));
    for (raw, phase) in [("warm", Phase::Warm), ("contention", Phase::Contention)] {
        assert_eq!(
            args(&["--fixtures", "d", "--phase", raw]).unwrap().phase,
            phase
        );
    }
    assert!(args(&["--fixtures", "d", "--cache"]).is_err());
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
fn opcion_de_huella() {
    assert!(
        args(&["--fixtures", "d", "--fingerprint"])
            .unwrap()
            .fingerprint
    );
    assert!(!args(&["--fixtures", "d"]).unwrap().fingerprint);
}

fn ms_list(range: std::ops::RangeInclusive<u64>) -> Vec<std::time::Duration> {
    range.map(std::time::Duration::from_millis).collect()
}

fn tramos(range: std::ops::RangeInclusive<u64>) -> Option<(u64, u64)> {
    refresh_segments(&ms_list(range)).map(|(a, b)| (a.as_millis() as u64, b.as_millis() as u64))
}

#[test]
fn medianas_de_los_primeros_y_ultimos_20_refrescos() {
    assert_eq!(tramos(1..=40), Some((11, 31)));
    assert_eq!(tramos(1..=200), Some((11, 191)));
    let mut desordenado = ms_list(1..=40);
    desordenado.reverse();
    let (a, b) = refresh_segments(&desordenado).unwrap();
    assert_eq!((a.as_millis(), b.as_millis()), (31, 11));
}

#[test]
fn con_menos_de_40_refrescos_usa_las_mitades() {
    assert_eq!(tramos(1..=39), Some((10, 30)));
    assert_eq!(tramos(1..=10), Some((3, 8)));
    assert_eq!(tramos(1..=11), Some((3, 9)));
    assert_eq!(tramos(1..=2), Some((1, 2)));
    assert_eq!(tramos(1..=1), None);
    assert_eq!(refresh_segments(&[]), None);
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

// ------------------------------------------------------------------ golden de consultas (D1)

use common::{canonical_line, compare_golden, compare_json, value_hash};
use serde_json::json;

#[test]
fn enteros_cadenas_y_booleanos_se_comparan_exactos() {
    let base = json!({"n": 3, "s": "x", "b": true, "z": null});
    assert!(compare_json(&base, &base.clone()).is_ok());
    for otro in [
        json!({"n": 4, "s": "x", "b": true, "z": null}),
        json!({"n": 3, "s": "y", "b": true, "z": null}),
        json!({"n": 3, "s": "x", "b": false, "z": null}),
        json!({"n": 3, "s": "x", "b": true, "z": 0}),
        json!({"n": 3.0, "s": "x", "b": true, "z": null}),
    ] {
        assert!(compare_json(&base, &otro).is_err(), "{otro}");
    }
}

#[test]
fn reales_con_tolerancia_de_una_millonesima() {
    for (a, b) in [
        (0.5, 0.500001),
        (1.0, 1.000001),
        (2.0, 1.999999),
        (0.0, -0.0),
    ] {
        assert!(
            compare_json(&json!({"c": a}), &json!({"c": b})).is_ok(),
            "{a} {b}"
        );
    }
    for (a, b) in [(0.5, 0.500002), (1.0, 0.999998), (1e6, 1e6 + 0.01)] {
        assert!(
            compare_json(&json!({"c": a}), &json!({"c": b})).is_err(),
            "{a} {b}"
        );
    }
}

#[test]
fn claves_y_elementos_de_mas_o_de_menos_fallan() {
    let base = json!({"a": 1, "l": [1, 2]});
    for otro in [
        json!({"a": 1}),
        json!({"a": 1, "l": [1, 2], "x": 0}),
        json!({"l": [1, 2], "b": 1}),
        json!({"a": 1, "l": [1]}),
        json!({"a": 1, "l": [1, 2, 3]}),
        json!({"a": 1, "l": [2, 1]}),
    ] {
        let err = compare_json(&base, &otro).unwrap_err();
        assert!(!err.is_empty(), "{otro}");
    }
    let anidado = |c: f64| json!([{"k": "a", "v": [{"c": c}]}]);
    assert!(compare_json(&anidado(1.0), &anidado(1.0000004)).is_ok());
    assert!(compare_json(&anidado(1.0), &anidado(1.1))
        .unwrap_err()
        .contains("[0].v[0].c"));
}

#[test]
fn la_linea_canonica_no_depende_del_orden_de_las_claves() {
    let a: serde_json::Value =
        serde_json::from_str(r#"{"b":1,"a":{"y":2,"x":[3,{"q":1,"p":2}]}}"#).unwrap();
    let b: serde_json::Value =
        serde_json::from_str(r#"{"a":{"x":[3,{"p":2,"q":1}],"y":2},"b":1}"#).unwrap();
    let la = canonical_line("summary", "todo", &a);
    assert_eq!(la, canonical_line("summary", "todo", &b));
    assert_eq!(
        la,
        r#"summary	todo	{"a":{"x":[3,{"p":2,"q":1}],"y":2},"b":1}"#
    );
    assert!(!la.contains('\n'));
}

#[test]
fn el_hash_redondea_los_reales_a_una_millonesima() {
    let h = |v: serde_json::Value| value_hash(&v);
    assert_eq!(h(json!({"c": 1.0000001})), h(json!({"c": 1.0000002})));
    assert_eq!(h(json!({"c": 0.0})), h(json!({"c": -0.0000001})));
    assert_ne!(h(json!({"c": 1.0})), h(json!({"c": 1.00001})));
    assert_ne!(h(json!({"c": 1})), h(json!({"c": 2})));
    assert_ne!(h(json!({"c": "a"})), h(json!({"d": "a"})));
    assert_eq!(
        h(serde_json::from_str(r#"{"a":1,"b":2}"#).unwrap()),
        h(serde_json::from_str(r#"{"b":2,"a":1}"#).unwrap())
    );
}

#[test]
fn el_golden_compara_linea_a_linea_por_nombre() {
    let esperado = "summary\ttodo\t{\"c\":1.0}\nbreakdown(agent)\ttodo\t[]\n";
    assert!(compare_golden(
        esperado,
        "summary\ttodo\t{\"c\":1.0000001}\nbreakdown(agent)\ttodo\t[]\n"
    )
    .is_ok());
    for actual in [
        "summary\ttodo\t{\"c\":1.1}\nbreakdown(agent)\ttodo\t[]\n",
        "summary\ttodo\t{\"c\":1.0}\n",
        "summary\ttodo\t{\"c\":1.0}\nbreakdown(model)\ttodo\t[]\n",
        "summary\ttodo\t{\"c\":1.0}\nbreakdown(agent)\ttodo\t[]\nx\ty\t1\n",
    ] {
        let err = compare_golden(esperado, actual).unwrap_err();
        assert!(!err.is_empty(), "{actual}");
    }
}
