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
/// 2026-08-01T00:00 en Madrid (2026-07-31T22:00:00Z): el «mes en curso» del Resumen para los
/// fixtures, en la medianoche local que manda la interfaz.
pub const MONTH_START_MS: i64 = WINDOW_END_MS - 31 * DAY_MS - 2 * 3_600_000;
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

/// Últimos 30 días de la ventana de los fixtures (no depende de «ahora»), en medianoches de `TZ`
/// como los manda la interfaz: del 3 de agosto al 1 de septiembre de 2026 incluidos.
pub fn last_30_days() -> queries::Filter {
    let zone = agentboard_lib::tz::parse(TZ);
    let to = agentboard_lib::tz::day_start(WINDOW_END_MS + DAY_MS, &zone);
    queries::Filter {
        from: Some(agentboard_lib::tz::day_start(
            to - 30 * DAY_MS + 6 * 3_600_000,
            &zone,
        )),
        to: Some(to),
        ..Default::default()
    }
}

/// Abre la base de medición con la zona de la caché en `TZ`, como la app con la zona del
/// sistema igual a la de la interfaz (así los periodos de días completos leen de los agregados).
pub fn open_measure_db() -> Result<Connection> {
    let conn = agentboard_lib::db::open_in_memory()?;
    agentboard_lib::db::set_cache_tz(&conn, TZ)?;
    Ok(conn)
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

/// El mes en curso de los fixtures con los agentes y proyectos de `f`.
fn month_of(f: &queries::Filter) -> queries::Filter {
    queries::Filter {
        from: Some(MONTH_START_MS),
        to: None,
        ..f.clone()
    }
}

/// Las 18 consultas del Resumen (`full_summary`), una a una y con su nombre.
pub fn resumen_queries() -> Vec<(&'static str, Query)> {
    vec![
        ("summary", |c, f| {
            queries::summary(c, f, WINDOW_END_MS).map(|_| ())
        }),
        ("breakdown(agent)", |c, f| {
            queries::breakdown(c, f, "agent").map(|_| ())
        }),
        ("timeseries(day)", |c, f| {
            queries::timeseries(c, f, "day", TZ).map(|_| ())
        }),
        ("timeseries(hour)", |c, f| {
            queries::timeseries(c, f, "hour", TZ).map(|_| ())
        }),
        ("timeseries_by(agent)", |c, f| {
            queries::timeseries_by(c, f, "agent", TZ).map(|_| ())
        }),
        ("timeseries_by(model)", |c, f| {
            queries::timeseries_by(c, f, "model", TZ).map(|_| ())
        }),
        ("timeseries_by(project)", |c, f| {
            queries::timeseries_by(c, f, "project", TZ).map(|_| ())
        }),
        ("timeseries_by(tool)", |c, f| {
            queries::timeseries_by(c, f, "tool", TZ).map(|_| ())
        }),
        ("timeseries(day) del mes", |c, f| {
            queries::timeseries(c, &month_of(f), "day", TZ).map(|_| ())
        }),
        ("breakdown(project)", |c, f| {
            queries::breakdown(c, f, "project").map(|_| ())
        }),
        ("breakdown(model)", |c, f| {
            queries::breakdown(c, f, "model").map(|_| ())
        }),
        ("activity", |c, f| insights::activity(c, f).map(|_| ())),
        ("activity_daily", |c, f| {
            insights::activity_daily(c, f, TZ).map(|_| ())
        }),
        ("breakdown(tool)", |c, f| {
            queries::breakdown(c, f, "tool").map(|_| ())
        }),
        ("breakdown(command)", |c, f| {
            queries::breakdown(c, f, "command").map(|_| ())
        }),
        ("breakdown(skill)", |c, f| {
            queries::breakdown(c, f, "skill").map(|_| ())
        }),
        ("breakdown(mcp)", |c, f| {
            queries::breakdown(c, f, "mcp").map(|_| ())
        }),
        ("breakdown(agent_type)", |c, f| {
            queries::breakdown(c, f, "agent_type").map(|_| ())
        }),
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

/// El Resumen completo a través de la caché de resultados, con las mismas claves que los comandos.
pub fn full_summary_cached(
    conn: &Connection,
    cache: &agentboard_lib::result_cache::ResultCache,
    generation: &agentboard_lib::ingest_thread::DataGeneration,
    f: &queries::Filter,
) -> Result<usize> {
    use agentboard_lib::result_cache::{key, key_at_minute};
    let month = queries::Filter {
        from: Some(MONTH_START_MS),
        to: None,
        ..f.clone()
    };
    cache.get_or_compute(
        generation,
        key_at_minute("summary", f, WINDOW_END_MS),
        || queries::summary(conn, f, WINDOW_END_MS),
    )?;
    let breakdown = |by: &str| {
        cache.get_or_compute(generation, key("breakdown", &(f, by)), || {
            queries::breakdown(conn, f, by)
        })
    };
    let series = |f: &queries::Filter, bucket: &str| {
        cache.get_or_compute(generation, key("timeseries", &(f, bucket, TZ)), || {
            queries::timeseries(conn, f, bucket, TZ)
        })
    };
    breakdown("agent")?;
    series(f, "day")?;
    series(f, "hour")?;
    for by in ["agent", "model", "project", "tool"] {
        cache.get_or_compute(generation, key("timeseries_by", &(f, by, TZ)), || {
            queries::timeseries_by(conn, f, by, TZ)
        })?;
    }
    series(&month, "day")?;
    breakdown("project")?;
    breakdown("model")?;
    cache.get_or_compute(generation, key("activity", f), || {
        insights::activity(conn, f)
    })?;
    cache.get_or_compute(generation, key("activity_daily", &(f, TZ)), || {
        insights::activity_daily(conn, f, TZ)
    })?;
    for by in ["tool", "command", "skill", "mcp", "agent_type"] {
        breakdown(by)?;
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
    Warm,
    Contention,
    Watch,
    Idle,
    All,
}

/// Lector de `--phase contention`: el Resumen en bucle (Fase 2) o uno por aviso, como la interfaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    Loop,
    Ui,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub fixtures: PathBuf,
    pub phase: Phase,
    pub live_lines: usize,
    pub repeat: usize,
    pub fingerprint: bool,
    pub query_fingerprint: bool,
    pub cache: Option<PathBuf>,
    pub events: Option<usize>,
    pub idle_secs: usize,
    pub reader: Reader,
}

pub const USAGE: &str =
    "uso: perf_scan --fixtures <dir> [--phase cold|live|queries|warm|contention|watch|idle|all] [--live-lines N] [--repeat R] [--fingerprint] [--query-fingerprint] [--cache <ruta>] [--events N] [--idle-secs S] [--reader ui|loop]";

pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut fixtures = None;
    let mut phase = Phase::All;
    let mut live_lines = 1000;
    let mut repeat = 1;
    let mut fingerprint = false;
    let mut query_fingerprint = false;
    let mut cache = None;
    let mut events = None;
    let mut idle_secs = 60;
    let mut reader = Reader::Loop;
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
                    "warm" => Phase::Warm,
                    "contention" => Phase::Contention,
                    "watch" => Phase::Watch,
                    "idle" => Phase::Idle,
                    "all" => Phase::All,
                    other => return Err(format!("fase desconocida: {other}")),
                }
            }
            "--live-lines" => live_lines = positive(&flag, &value()?)?,
            "--repeat" => repeat = positive(&flag, &value()?)?,
            "--fingerprint" => fingerprint = true,
            "--query-fingerprint" => query_fingerprint = true,
            "--cache" => cache = Some(PathBuf::from(value()?)),
            "--events" => events = Some(positive(&flag, &value()?)?),
            "--idle-secs" => idle_secs = positive(&flag, &value()?)?,
            "--reader" => {
                reader = match value()?.as_str() {
                    "loop" => Reader::Loop,
                    "ui" => Reader::Ui,
                    other => return Err(format!("lector desconocido: {other}")),
                }
            }
            other => return Err(format!("argumento desconocido: {other}")),
        }
    }
    Ok(Args {
        fixtures: fixtures.ok_or("falta --fixtures <dir>")?,
        phase,
        live_lines,
        repeat,
        fingerprint,
        query_fingerprint,
        cache,
        events,
        idle_secs,
        reader,
    })
}

/// Separación mínima entre instantes consecutivos; `None` con menos de dos.
pub fn min_gap(at: &[std::time::Instant]) -> Option<Duration> {
    at.windows(2)
        .map(|w| w[1].saturating_duration_since(w[0]))
        .min()
}

/// Milisegundos por tick de `/proc/self/stat` (`CLK_TCK` = 100 en Linux x86-64 y arm64).
const MS_PER_TICK: u64 = 10;

fn cpu_ticks(stat: &str) -> Option<u64> {
    // El nombre del proceso (campo 2) va entre paréntesis y puede contener espacios: se cuenta
    // desde el último `)`, donde empieza el campo 3; utime y stime son los campos 14 y 15.
    let rest = &stat[stat.rfind(')')? + 1..];
    let mut fields = rest.split_whitespace().skip(11);
    let utime: u64 = fields.next()?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    Some(utime + stime)
}

/// CPU (usuario + sistema, en ms) consumida entre dos lecturas de `/proc/self/stat`.
pub fn cpu_ms(before: &str, after: &str) -> Option<u64> {
    Some(cpu_ticks(after)?.saturating_sub(cpu_ticks(before)?) * MS_PER_TICK)
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

// ------------------------------------------------------------------ golden de consultas (D1)

/// Diferencia máxima entre reales del golden de consultas: sumar por agregados cambia el orden de
/// las sumas en coma flotante.
pub const REAL_TOLERANCE: f64 = 1e-6;

fn sorted_value(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), sorted_value(&map[k]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted_value).collect()),
        other => other.clone(),
    }
}

fn canonical_json(v: &serde_json::Value) -> String {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let body: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{}:{}", Value::String(k.clone()), canonical_json(&map[k])))
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(items) => {
            let body: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", body.join(","))
        }
        other => other.to_string(),
    }
}

/// Una línea del golden: consulta, filtro y su salida en JSON con las claves ordenadas.
pub fn canonical_line(query: &str, filter: &str, value: &serde_json::Value) -> String {
    format!(
        "{query}\t{filter}\t{}",
        canonical_json(&sorted_value(value))
    )
}

fn compare_at(
    path: &str,
    expected: &serde_json::Value,
    actual: &serde_json::Value,
) -> Result<(), String> {
    use serde_json::Value;
    let differ = || Err(format!("{path}: esperado {expected}, obtenido {actual}"));
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            for k in e.keys() {
                if !a.contains_key(k) {
                    return Err(format!("{path}.{k}: falta la clave"));
                }
            }
            for k in a.keys() {
                if !e.contains_key(k) {
                    return Err(format!("{path}.{k}: clave de más"));
                }
            }
            for (k, ev) in e {
                compare_at(&format!("{path}.{k}"), ev, &a[k])?;
            }
            Ok(())
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                return Err(format!(
                    "{path}: {} elementos esperados, {} obtenidos",
                    e.len(),
                    a.len()
                ));
            }
            for (i, (ev, av)) in e.iter().zip(a).enumerate() {
                compare_at(&format!("{path}[{i}]"), ev, av)?;
            }
            Ok(())
        }
        (Value::Number(e), Value::Number(a)) => {
            if e.is_f64() != a.is_f64() {
                return differ();
            }
            if !e.is_f64() {
                return if e == a { Ok(()) } else { differ() };
            }
            let (x, y) = (
                e.as_f64().unwrap_or(f64::NAN),
                a.as_f64().unwrap_or(f64::NAN),
            );
            let slack = f64::EPSILON * x.abs().max(y.abs()).max(1.0);
            if (x - y).abs() <= REAL_TOLERANCE + slack {
                Ok(())
            } else {
                differ()
            }
        }
        _ if expected == actual => Ok(()),
        _ => differ(),
    }
}

/// Compara dos salidas de consulta: estructura, enteros, cadenas y booleanos exactos; reales con
/// `|a − b| ≤ 1e-6`. El error indica la ruta de la primera diferencia.
pub fn compare_json(
    expected: &serde_json::Value,
    actual: &serde_json::Value,
) -> Result<(), String> {
    compare_at("", expected, actual)
}

fn split_line(line: &str) -> Result<(&str, serde_json::Value), String> {
    let mut parts = line.splitn(3, '\t');
    let (Some(q), Some(f), Some(json)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(format!("línea mal formada: {line}"));
    };
    let name_len = q.len() + 1 + f.len();
    let value = serde_json::from_str(json).map_err(|e| format!("{q} {f}: {e}"))?;
    Ok((&line[..name_len], value))
}

/// Compara un golden de consultas con la salida actual, línea a línea y por nombre.
pub fn compare_golden(expected: &str, actual: &str) -> Result<(), String> {
    let (e_lines, a_lines): (Vec<&str>, Vec<&str>) =
        (expected.lines().collect(), actual.lines().collect());
    if e_lines.len() != a_lines.len() {
        return Err(format!(
            "{} líneas esperadas, {} obtenidas",
            e_lines.len(),
            a_lines.len()
        ));
    }
    let mut errors = Vec::new();
    for (e, a) in e_lines.iter().zip(&a_lines) {
        let (e_name, e_value) = split_line(e)?;
        let (a_name, a_value) = split_line(a)?;
        if e_name != a_name {
            return Err(format!("se esperaba {e_name:?} y llegó {a_name:?}"));
        }
        if let Err(diff) = compare_json(&e_value, &a_value) {
            errors.push(format!("{e_name}: {diff}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn hash_into(hash: u64, v: &serde_json::Value) -> u64 {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let h = keys.into_iter().fold(fnv1a(hash, b"{"), |h, k| {
                let h = fnv1a(fnv1a(h, k.as_bytes()), b":");
                fnv1a(hash_into(h, &map[k]), b",")
            });
            fnv1a(h, b"}")
        }
        Value::Array(items) => {
            let h = items
                .iter()
                .fold(fnv1a(hash, b"["), |h, x| fnv1a(hash_into(h, x), b","));
            fnv1a(h, b"]")
        }
        Value::Number(n) if n.is_f64() => {
            let rounded = (n.as_f64().unwrap_or(0.0) * 1e6).round() / 1e6;
            let rounded = if rounded == 0.0 { 0.0 } else { rounded };
            fnv1a(hash, format!("{rounded:.6}").as_bytes())
        }
        other => fnv1a(hash, other.to_string().as_bytes()),
    }
}

/// Hash FNV-1a de una salida de consulta con los reales redondeados a 1e-6 y las claves ordenadas.
pub fn value_hash(v: &serde_json::Value) -> u64 {
    hash_into(FNV_OFFSET, v)
}

/// Proveedores sobre los fixtures versionados (`tests/fixtures`); OpenCode necesita su base, que
/// se crea en `scratch` a partir del SQL versionado.
pub fn versioned_fixture_providers(scratch: &Path) -> Result<Vec<Box<dyn Provider>>> {
    use agentboard_lib::providers::opencode::OpenCode;
    let fx = manifest_dir().join("../tests/fixtures");
    let oc_root = scratch.join("opencode");
    std::fs::create_dir_all(&oc_root)?;
    let sql = std::fs::read_to_string(fx.join("opencode/opencode.sql"))?;
    Connection::open(oc_root.join("opencode.db"))?.execute_batch(&sql)?;
    Ok(vec![
        Box::new(ClaudeCode::with_roots(vec![fx.join("claude_code")])),
        Box::new(Codex::with_roots(vec![fx.join("codex/sessions")])),
        Box::new(Copilot::with_roots(vec![fx.join("copilot/session-state")])),
        Box::new(Cursor::with_roots(vec![fx.join("cursor/projects")])),
        Box::new(Gemini::with_roots(vec![fx.join("gemini/tmp")])),
        Box::new(OpenCode::with_roots(vec![oc_root])),
    ])
}

/// Genera el fixture sintético `size` con `seed` en `out`; `false` si no hay `python3`.
pub fn generate_fixture(size: &str, seed: u64, out: &Path) -> bool {
    let script = manifest_dir().join("../scripts/gen-fixtures.py");
    match Command::new("python3")
        .arg(script)
        .args([size, "--seed", &seed.to_string(), "--out"])
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

/// «Ahora» fijo (media hora después de la última llamada, para que el `burn rate` no sea 0) y
/// filtros del golden de consultas relativos al final de los datos (`end`, la medianoche local
/// siguiente a la última llamada), con nombre para que cada línea diga cuál es.
pub fn golden_filters(conn: &Connection) -> Result<(i64, Vec<(&'static str, queries::Filter)>)> {
    use chrono::{Datelike, TimeZone};
    let zone = agentboard_lib::tz::parse(TZ);
    let last: Option<i64> = conn.query_row("SELECT MAX(ts) FROM calls", [], |r| r.get(0))?;
    let last = last.unwrap_or(WINDOW_END_MS);
    let midnight = |ts: i64| agentboard_lib::tz::day_start(ts, &zone);
    let end = midnight(last + DAY_MS + 6 * 3_600_000);
    let days_back = |n: i64| midnight(end - n * DAY_MS + 6 * 3_600_000);
    let local_last = zone
        .timestamp_millis_opt(end - 1)
        .single()
        .ok_or_else(|| anyhow::anyhow!("fecha local inválida"))?;
    let month_start = zone
        .with_ymd_and_hms(local_last.year(), local_last.month(), 1, 0, 0, 0)
        .earliest()
        .map(|d| d.timestamp_millis())
        .ok_or_else(|| anyhow::anyhow!("inicio de mes inválido"))?;
    let project: Option<i64> = conn
        .query_row(
            "SELECT p.id FROM projects p JOIN sessions s ON s.project_id = p.id
             GROUP BY p.id ORDER BY COUNT(*) DESC, p.cwd LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();
    let base = queries::Filter::default();
    Ok((
        last + 1_800_000,
        vec![
            ("todo", base.clone()),
            (
                "30d",
                queries::Filter {
                    from: Some(days_back(30)),
                    to: Some(end),
                    ..base.clone()
                },
            ),
            (
                "mes",
                queries::Filter {
                    from: Some(month_start),
                    to: None,
                    ..base.clone()
                },
            ),
            (
                "7d+agente",
                queries::Filter {
                    from: Some(days_back(7)),
                    to: Some(end),
                    agents: Some(vec!["claude-code".into()]),
                    ..base.clone()
                },
            ),
            (
                "proyecto",
                queries::Filter {
                    projects: Some(project.into_iter().collect()),
                    ..base.clone()
                },
            ),
            (
                "sin-proyecto",
                queries::Filter {
                    no_project: true,
                    ..base.clone()
                },
            ),
            (
                "anterior",
                queries::Filter {
                    from: Some(days_back(60)),
                    to: Some(days_back(30)),
                    ..base.clone()
                },
            ),
            (
                "mitad-de-dia",
                queries::Filter {
                    from: Some(days_back(10) + 13 * 3_600_000 + 1_234),
                    to: Some(end),
                    ..base
                },
            ),
        ],
    ))
}

fn json_of<T: serde::Serialize>(v: T) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(v)?)
}

/// Salidas de todas las consultas del Resumen y de los apartados para los filtros del golden,
/// como `(consulta, filtro, salida)` en un orden fijo.
pub fn query_outputs(conn: &Connection) -> Result<Vec<(String, String, serde_json::Value)>> {
    use agentboard_lib::{pricing, sessions};
    let (now, filters) = golden_filters(conn)?;
    let mut out = Vec::new();
    for (name, f) in &filters {
        let mut push =
            |q: &str, v: serde_json::Value| out.push((q.to_string(), name.to_string(), v));
        push("summary", json_of(queries::summary(conn, f, now)?)?);
        for bucket in ["day", "hour"] {
            push(
                &format!("timeseries({bucket})"),
                json_of(queries::timeseries(conn, f, bucket, TZ)?)?,
            );
        }
        for by in ["agent", "model", "project", "branch", "tool"] {
            push(
                &format!("timeseries_by({by})"),
                json_of(queries::timeseries_by(conn, f, by, TZ)?)?,
            );
        }
        for by in [
            "agent",
            "project",
            "branch",
            "model",
            "activity",
            "tool",
            "command",
            "skill",
            "mcp",
            "agent_type",
        ] {
            push(
                &format!("breakdown({by})"),
                json_of(queries::breakdown(conn, f, by)?)?,
            );
        }
        push("activity", json_of(insights::activity(conn, f)?)?);
        push(
            "activity_daily",
            json_of(insights::activity_daily(conn, f, TZ)?)?,
        );
        push("list_agents", json_of(queries::list_agents(conn, f)?)?);
        push("list_projects", json_of(queries::list_projects(conn, f)?)?);
        push("list_prices", json_of(pricing::list_prices(conn, f, &[])?)?);
        push(
            "list_sessions",
            json_of(sessions::list_sessions(conn, f, None)?)?,
        );
        push(
            "sessions::list_projects",
            json_of(sessions::list_projects(conn, f)?)?,
        );
    }
    Ok(out)
}

/// Golden de consultas: una línea canónica por (consulta, filtro).
pub fn query_golden(conn: &Connection) -> Result<String> {
    Ok(query_outputs(conn)?
        .iter()
        .map(|(q, f, v)| canonical_line(q, f, v) + "\n")
        .collect())
}

/// Huella de consultas (`perf_scan --query-fingerprint`): un hash por (consulta, filtro).
pub fn query_fingerprint(conn: &Connection) -> Result<String> {
    Ok(query_outputs(conn)?
        .iter()
        .map(|(q, f, v)| format!("{q} {f}: {:016x}\n", value_hash(v)))
        .collect())
}
