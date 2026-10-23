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
