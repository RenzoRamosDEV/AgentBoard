//! Piezas compartidas por los benchmarks (`benches/`) y el binario de medición
//! (`examples/perf_scan.rs`): fixtures sintéticos, proveedores con raíces inyectadas (nunca los
//! logs reales), el filtro fijo de 30 días y las consultas del Resumen.

#![allow(dead_code)]

use agentboard_lib::providers::{
    claude_code::ClaudeCode, codex::Codex, copilot::Copilot, cursor::Cursor, gemini::Gemini,
    Provider,
};
use agentboard_lib::{insights, queries};
use anyhow::Result;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Fin de la ventana temporal de los fixtures (`WINDOW_END` en `scripts/gen-fixtures.py`).
pub const WINDOW_END_MS: i64 = 1_788_220_800_000; // 2026-09-01T00:00:00Z
pub const DAY_MS: i64 = 86_400_000;
/// 2026-08-01T00:00:00Z: el «mes en curso» del Resumen para los fixtures.
pub const MONTH_START_MS: i64 = WINDOW_END_MS - 31 * DAY_MS;
pub const TZ: &str = "Europe/Madrid";

pub fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixtures_dir(size: &str) -> PathBuf {
    manifest_dir().join("target/perf-fixtures").join(size)
}

pub fn gen_command(size: &str) -> String {
    format!("python3 scripts/gen-fixtures.py {size}")
}

/// Carpeta del fixture `size`; si no existe intenta generarla. `None` con un mensaje si no se pudo.
pub fn ensure_fixture(size: &str) -> Option<PathBuf> {
    let dir = fixtures_dir(size);
    if dir.is_dir() {
        return Some(dir);
    }
    let script = manifest_dir().join("../scripts/gen-fixtures.py");
    let generated = Command::new("python3")
        .arg(script)
        .arg(size)
        .status()
        .is_ok_and(|s| s.success());
    if generated && dir.is_dir() {
        return Some(dir);
    }
    eprintln!(
        "AVISO: no hay fixture {size} en {} y no se pudo generar. Genéralo desde la raíz del repo con:\n  {}",
        dir.display(),
        gen_command(size)
    );
    None
}

/// Los cinco proveedores JSONL apuntando solo a las subcarpetas del fixture.
pub fn providers(root: &Path) -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(ClaudeCode::with_roots(vec![root.join("claude_code")])),
        Box::new(Codex::with_roots(vec![root.join("codex/sessions")])),
        Box::new(Copilot::with_roots(
            vec![root.join("copilot/session-state")],
        )),
        Box::new(Cursor::with_roots(vec![root.join("cursor/projects")])),
        Box::new(Gemini::with_roots(vec![root.join("gemini/tmp")])),
    ]
}

/// Últimos 30 días de la ventana de los fixtures (no depende de «ahora»).
pub fn last_30_days() -> queries::Filter {
    queries::Filter {
        from: Some(WINDOW_END_MS - 30 * DAY_MS),
        to: Some(WINDOW_END_MS),
        ..Default::default()
    }
}

/// Las 5 consultas más usadas del Resumen, con su nombre.
pub fn top_queries() -> Vec<(
    &'static str,
    fn(&Connection, &queries::Filter) -> Result<()>,
)> {
    vec![
        ("summary", |c, f| {
            queries::summary(c, f, WINDOW_END_MS).map(|_| ())
        }),
        ("breakdown(project)", |c, f| {
            queries::breakdown(c, f, "project").map(|_| ())
        }),
        ("breakdown(model)", |c, f| {
            queries::breakdown(c, f, "model").map(|_| ())
        }),
        ("timeseries(day)", |c, f| {
            queries::timeseries(c, f, "day", TZ).map(|_| ())
        }),
        ("activity", |c, f| insights::activity(c, f).map(|_| ())),
    ]
}

/// La llamada completa del Resumen (`useDashboardData`, sin periodo anterior), en secuencia como
/// la ejecutan los comandos con el `Mutex` único de la base. Devuelve el número de consultas.
pub fn full_summary(conn: &Connection, f: &queries::Filter) -> Result<usize> {
    let month = queries::Filter {
        from: Some(MONTH_START_MS),
        to: None,
        ..f.clone()
    };
    queries::summary(conn, f, WINDOW_END_MS)?;
    queries::breakdown(conn, f, "agent")?;
    queries::timeseries(conn, f, "day", TZ)?;
    queries::timeseries(conn, f, "hour", TZ)?;
    for by in ["agent", "model", "project", "tool"] {
        queries::timeseries_by(conn, f, by, TZ)?;
    }
    queries::timeseries(conn, &month, "day", TZ)?;
    queries::breakdown(conn, f, "project")?;
    queries::breakdown(conn, f, "model")?;
    insights::activity(conn, f)?;
    insights::activity_daily(conn, f, TZ)?;
    for by in ["tool", "command", "skill", "mcp", "agent_type"] {
        queries::breakdown(conn, f, by)?;
    }
    Ok(18)
}
