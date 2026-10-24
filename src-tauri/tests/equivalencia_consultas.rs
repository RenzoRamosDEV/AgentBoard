//! Equivalencia de las consultas: las salidas de todas las consultas del Resumen y de los
//! apartados sobre los fixtures sintético (`2m`, semilla 11) y versionados coinciden con los golden
//! grabados antes de optimizar la base (enteros exactos, reales con tolerancia 1e-6).
//! `UPDATE_GOLDEN=1 cargo test --test equivalencia_consultas` los regraba.

#[path = "../benches/common/mod.rs"]
mod common;

use agentboard_lib::providers::Provider;
use agentboard_lib::{db, ingest};

fn check(name: &str, actual: &str) {
    let path = common::manifest_dir().join("tests/golden").join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "falta {}: grábalo con UPDATE_GOLDEN=1 en el código de referencia",
            path.display()
        )
    });
    if let Err(diff) = common::compare_golden(&expected, actual) {
        panic!("las consultas de {name} cambiaron:\n{diff}");
    }
}

/// Salida de las consultas con las carpetas volátiles (temporales y del repo) sustituidas, porque
/// `list_agents` devuelve la raíz de logs de cada agente.
fn golden_of(providers: &[Box<dyn Provider>], volatile: &[&std::path::Path]) -> String {
    let mut conn = db::open_in_memory().unwrap();
    let stats = ingest::scan_all(&mut conn, providers).unwrap();
    assert_eq!(stats.errors, 0, "{stats:?}");
    let mut out = common::query_golden(&conn).unwrap();
    for (i, dir) in volatile.iter().enumerate() {
        out = out.replace(&dir.display().to_string(), &format!("<raiz{i}>"));
    }
    out
}

#[test]
fn consultas_del_fixture_2m_iguales_que_el_golden() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("2m");
    if !common::generate_fixture("2m", 11, &root) {
        eprintln!("python3 no disponible: se salta la equivalencia de consultas");
        return;
    }
    check(
        "consultas_2m.txt",
        &golden_of(&common::providers(&root), &[dir.path()]),
    );
}

#[test]
fn consultas_de_los_fixtures_versionados_iguales_que_el_golden() {
    let dir = tempfile::tempdir().unwrap();
    let providers = common::versioned_fixture_providers(dir.path()).unwrap();
    let repo = common::manifest_dir().join("../tests/fixtures");
    check(
        "consultas_fixtures.txt",
        &golden_of(&providers, &[dir.path(), &repo]),
    );
}
