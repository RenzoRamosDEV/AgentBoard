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
use std::time::Duration;

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

pub type Query = fn(&Connection, &queries::Filter) -> Result<()>;

/// Las 5 consultas más usadas del Resumen, con su nombre.
pub fn top_queries() -> Vec<(&'static str, Query)> {
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

// ------------------------------------------------------------------ huella

/// Por tabla: columnas comparables (sin ids autoincrementales ni marcas de reloj) y clave natural.
const FINGERPRINT_TABLES: &[(&str, &str)] = &[
    (
        "calls",
        "SELECT message_id, session_id, ts, model, input_tokens, output_tokens, cache_read,
           cache_write, cache_write_1h, reasoning_tokens, activity, turn_id, is_sidechain,
           agent_id, cost_reported FROM calls ORDER BY message_id",
    ),
    (
        "tool_calls",
        "SELECT call_id, message_id, session_id, ts, tool, target, is_error, duration_ms, turn_id,
           detail, agent_id FROM tool_calls ORDER BY call_id",
    ),
    (
        "turns",
        "SELECT id, session_id, ts, intent FROM turns ORDER BY id",
    ),
    (
        "sessions",
        "SELECT s.id, s.agent_id, p.cwd, s.git_branch, s.started_at, s.ended_at, s.model,
           s.is_subagent FROM sessions s LEFT JOIN projects p ON p.id = s.project_id ORDER BY s.id",
    ),
    (
        "projects",
        "SELECT name, cwd, repo_root FROM projects ORDER BY cwd",
    ),
    (
        "events",
        "SELECT session_id, ts, kind, payload_json FROM events ORDER BY session_id, ts, kind",
    ),
];

const PER_AGENT: &str = "SELECT a.id,
    (SELECT COUNT(*) FROM sessions s WHERE s.agent_id = a.id),
    (SELECT COUNT(*) FROM calls c JOIN sessions s ON s.id = c.session_id WHERE s.agent_id = a.id),
    (SELECT COUNT(*) FROM tool_calls t JOIN sessions s ON s.id = t.session_id WHERE s.agent_id = a.id),
    (SELECT COUNT(*) FROM turns t JOIN sessions s ON s.id = t.session_id WHERE s.agent_id = a.id),
    (SELECT COUNT(*) FROM events e JOIN sessions s ON s.id = e.session_id WHERE s.agent_id = a.id),
    (SELECT printf('%d/%d/%d/%d/%d/%d', TOTAL(input_tokens), TOTAL(output_tokens), TOTAL(cache_read),
       TOTAL(cache_write), TOTAL(cache_write_1h), TOTAL(reasoning_tokens))
       FROM calls c JOIN sessions s ON s.id = c.session_id WHERE s.agent_id = a.id),
    (SELECT printf('%.9f', TOTAL(cost_usd)) FROM call_costs c JOIN sessions s ON s.id = c.session_id
       WHERE s.agent_id = a.id)
  FROM agents a ORDER BY a.id";

/// FNV-1a de 64 bits: estable entre versiones de Rust (a diferencia de `DefaultHasher`).
fn fnv1a(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

fn render(v: rusqlite::types::ValueRef<'_>) -> String {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Null => "\u{2205}".to_string(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => format!("{f:.9}"),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
        ValueRef::Blob(b) => format!("{b:02x?}"),
    }
}

/// Huella canónica de los datos importados: conteos, tokens y coste por agente, y un hash por
/// tabla de sus filas ordenadas por clave natural. Igual huella = mismos datos importados.
pub fn fingerprint(conn: &Connection) -> Result<String> {
    let mut out = String::new();
    let mut stmt = conn.prepare(PER_AGENT)?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        out.push_str(&format!(
            "agente {}: sesiones {}, llamadas {}, herramientas {}, turnos {}, eventos {}, tokens {}, coste {}\n",
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, String>(7)?,
        ));
    }
    for (table, sql) in FINGERPRINT_TABLES {
        let mut stmt = conn.prepare(sql)?;
        let columns = stmt.column_count();
        let mut rows = stmt.query([])?;
        let (mut hash, mut count) = (FNV_OFFSET, 0u64);
        while let Some(r) = rows.next()? {
            for i in 0..columns {
                hash = fnv1a(hash, render(r.get_ref(i)?).as_bytes());
                hash = fnv1a(hash, b"\x1f");
            }
            hash = fnv1a(hash, b"\n");
            count += 1;
        }
        out.push_str(&format!("tabla {table}: {count} filas, fnv {hash:016x}\n"));
    }
    Ok(out)
}

// ------------------------------------------------------------------ perf_scan

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Cold,
    Live,
    Queries,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub fixtures: PathBuf,
    pub phase: Phase,
    pub live_lines: usize,
    pub repeat: usize,
    pub fingerprint: bool,
}

pub const USAGE: &str =
    "uso: perf_scan --fixtures <dir> [--phase cold|live|queries|all] [--live-lines N] [--repeat R] [--fingerprint]";

pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut fixtures = None;
    let mut phase = Phase::All;
    let mut live_lines = 1000;
    let mut repeat = 1;
    let mut fingerprint = false;
    let mut it = args.into_iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("falta el valor de {flag}"));
        match flag.as_str() {
            "--fixtures" => fixtures = Some(PathBuf::from(value()?)),
            "--phase" => {
                phase = match value()?.as_str() {
                    "cold" => Phase::Cold,
                    "live" => Phase::Live,
                    "queries" => Phase::Queries,
                    "all" => Phase::All,
                    other => return Err(format!("fase desconocida: {other}")),
                }
            }
            "--live-lines" => live_lines = positive(&flag, &value()?)?,
            "--repeat" => repeat = positive(&flag, &value()?)?,
            "--fingerprint" => fingerprint = true,
            other => return Err(format!("argumento desconocido: {other}")),
        }
    }
    Ok(Args {
        fixtures: fixtures.ok_or("falta --fixtures <dir>")?,
        phase,
        live_lines,
        repeat,
        fingerprint,
    })
}

pub fn median(v: &[Duration]) -> Duration {
    let mut v = v.to_vec();
    v.sort();
    v[v.len() / 2]
}

/// Refrescos que forman cada tramo de `refresh_segments`.
pub const REFRESH_SEGMENT: usize = 20;

/// Medianas del primer y del último tramo de refrescos (en el orden en que se midieron): los 20
/// primeros y los 20 últimos, o las dos mitades si hay menos de 40. `None` con menos de 2.
pub fn refresh_segments(times: &[Duration]) -> Option<(Duration, Duration)> {
    let n = (times.len() / 2).min(REFRESH_SEGMENT);
    if n == 0 {
        return None;
    }
    Some((median(&times[..n]), median(&times[times.len() - n..])))
}

fn positive(flag: &str, raw: &str) -> Result<usize, String> {
    match raw.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!("{flag} necesita un entero positivo, no {raw:?}")),
    }
}

/// Memoria residente pico (`VmHWM`, en KiB) de un texto con el formato de `/proc/self/status`.
pub fn vm_hwm_kib(status: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.trim().strip_suffix("kB"))
        .and_then(|n| n.trim().parse().ok())
}

/// `VmHWM` del proceso actual; `None` fuera de Linux.
pub fn peak_rss_kib() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .as_deref()
        .and_then(vm_hwm_kib)
}

/// Archivo de actividad simulada: se borra al soltarse, también si la medición falla.
pub struct LiveFile {
    pub path: PathBuf,
}

impl LiveFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for LiveFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// `n` líneas nuevas de Claude Code (ids únicos a partir de `start`), terminadas en salto de línea.
pub fn live_lines(session: &str, start: usize, n: usize) -> String {
    let mut out = String::with_capacity(n * 600);
    for i in start..start + n {
        let secs = i % 60;
        let mins = (i / 60) % 60;
        out.push_str(&format!(
            r#"{{"type":"assistant","timestamp":"2026-08-31T23:{mins:02}:{secs:02}.000Z","cwd":"/home/dev/Proyectos/proyecto-00","sessionId":"{session}","version":"2.1.282","gitBranch":"main","isSidechain":false,"message":{{"model":"claude-sonnet-4-5","id":"msg_live_{session}_{i}","type":"message","role":"assistant","content":[{{"type":"tool_use","id":"toolu_live_{session}_{i}","name":"Bash","input":{{"command":"cargo test"}}}}],"usage":{{"input_tokens":10,"cache_creation_input_tokens":0,"cache_read_input_tokens":20000,"output_tokens":120}}}}}}"#
        ));
        out.push('\n');
    }
    out
}
