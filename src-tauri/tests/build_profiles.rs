//! Perfiles de compilación del workspace: `release` optimizado al máximo y `profiling` con símbolos.

fn section(manifest: &str, name: &str) -> Vec<(String, String)> {
    let header = format!("[{name}]");
    let mut inside = false;
    let mut keys = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == header;
            continue;
        }
        if !inside || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            keys.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    keys
}

fn manifest() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("Cargo.toml legible")
}

fn assert_keys(name: &str, expected: &[(&str, &str)]) {
    let keys = section(&manifest(), name);
    for (k, v) in expected {
        assert!(
            keys.iter().any(|(kk, vv)| kk == k && vv == v),
            "[{name}] debe tener {k} = {v}; tiene {keys:?}"
        );
    }
}

#[test]
fn perfil_release_optimizado() {
    assert_keys(
        "profile.release",
        &[
            ("lto", "\"fat\""),
            ("codegen-units", "1"),
            ("opt-level", "3"),
            ("panic", "\"abort\""),
            ("strip", "true"),
        ],
    );
}

#[test]
fn perfil_profiling_hereda_con_simbolos() {
    assert_keys(
        "profile.profiling",
        &[
            ("inherits", "\"release\""),
            ("debug", "true"),
            ("strip", "false"),
        ],
    );
}

#[test]
fn perfil_bench_fast_sin_lto() {
    assert_keys(
        "profile.bench-fast",
        &[
            ("inherits", "\"release\""),
            ("lto", "false"),
            ("codegen-units", "16"),
        ],
    );
}

#[test]
fn el_parser_de_secciones_no_mezcla_secciones() {
    let toml = "[a]\nx = 1\n# c\n[b]\nx = 2\n";
    assert_eq!(section(toml, "a"), vec![("x".into(), "1".into())]);
    assert_eq!(section(toml, "b"), vec![("x".into(), "2".into())]);
    assert!(section(toml, "c").is_empty());
}
