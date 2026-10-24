//! Lectura incremental de logs: offset por archivo, upsert por id y nada se borra solo.

use crate::providers::{Provider, Record, Source};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tracing::field::Empty;

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStats {
    pub files: usize,
    pub lines: usize,
    pub records: usize,
    pub errors: usize,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct FileResult {
    pub lines: usize,
    pub records: usize,
    pub bad_lines: usize,
    /// Tool calls nuevas de esta pasada (para el feed en vivo).
    pub new_tool_calls: Vec<String>,
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Registra cada agente instalado o con carpeta de logs (aunque aún no tenga sesiones).
pub fn register_agents(conn: &Connection, providers: &[Box<dyn Provider>]) -> Result<()> {
    for p in providers {
        let roots = p.log_roots();
        let existing = roots.iter().find(|r| r.is_dir());
        if existing.is_none() && !p.installed() {
            continue;
        }
        let root = existing
            .or(roots.first())
            .map(|r| r.to_string_lossy().to_string())
            .unwrap_or_default();
        conn.execute(
            "INSERT INTO agents (id, name, log_root, first_seen) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET log_root = excluded.log_root",
            params![p.id(), p.name(), root, now_ms()],
        )?;
    }
    Ok(())
}

/// Garantiza la fila del agente (las sesiones la referencian).
fn ensure_agent(conn: &Connection, p: &dyn Provider) -> Result<()> {
    let root = p
        .log_roots()
        .first()
        .map(|r| r.to_string_lossy().to_string())
        .unwrap_or_default();
    conn.prepare_cached(
        "INSERT OR IGNORE INTO agents (id, name, log_root, first_seen) VALUES (?1, ?2, ?3, ?4)",
    )?
    .execute(params![p.id(), p.name(), root, now_ms()])?;
    Ok(())
}

/// Escaneo completo: importa todo lo que haya en disco (solo lo nuevo si ya se importó).
#[tracing::instrument(level = "info", skip_all, fields(files = Empty, lines = Empty, records = Empty, errors = Empty))]
pub fn scan_all(conn: &mut Connection, providers: &[Box<dyn Provider>]) -> Result<ScanStats> {
    register_agents(conn, providers)?;
    let mut stats = ScanStats::default();
    // Proyectos creados por un `cwd` pasajero (versiones anteriores) que ninguna sesión usa.
    conn.execute("DELETE FROM projects WHERE id NOT IN (SELECT project_id FROM sessions WHERE project_id IS NOT NULL)", [])?;
    let states = load_file_states(conn)?;
    for p in providers {
        for root in p.log_roots().into_iter().filter(|r| r.is_dir()) {
            for path in walk(&root) {
                if !p.matches(&path) {
                    continue;
                }
                let prev = states.get(path.to_string_lossy().as_ref());
                match ingest_file_from(conn, p.as_ref(), &path, prev) {
                    Ok(r) => {
                        stats.files += 1;
                        stats.lines += r.lines;
                        stats.records += r.records;
                    }
                    Err(e) => {
                        stats.errors += 1;
                        tracing::warn!(ruta = %path.display(), "agentboard: error leyendo {}: {e:#}", path.display());
                    }
                }
            }
        }
    }
    let span = tracing::Span::current();
    span.record("files", stats.files);
    span.record("lines", stats.lines);
    span.record("records", stats.records);
    span.record("errors", stats.errors);
    Ok(stats)
}

/// Archivos bajo `root`, recursivo, sin seguir enlaces.
#[tracing::instrument(level = "debug", skip_all, fields(raiz = %root.display(), files = Empty))]
pub fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => out.push(e.path()),
                _ => {}
            }
        }
    }
    out.sort();
    tracing::Span::current().record("files", out.len());
    out
}

#[cfg(unix)]
fn file_identity(meta: &fs::Metadata) -> String {
    use std::os::unix::fs::MetadataExt;
    format!("{}:{}", meta.dev(), meta.ino())
}

#[cfg(not(unix))]
fn file_identity(_meta: &fs::Metadata) -> String {
    // En Windows se detecta la rotación solo por tamaño (el archivo encoge).
    String::new()
}

fn mtime_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Lo guardado de un archivo en la pasada anterior.
#[derive(Debug, Clone, PartialEq)]
pub struct FileState {
    pub file_id: String,
    pub offset: i64,
}

const SELECT_FILE_STATES: &str = "SELECT path, file_id, offset FROM file_state";
const SELECT_FILE_STATE: &str = "SELECT file_id, offset FROM file_state WHERE path = ?1";

/// Estado de todos los archivos leídos, en una sola consulta.
pub fn load_file_states(conn: &Connection) -> Result<HashMap<String, FileState>> {
    let mut stmt = conn.prepare_cached(SELECT_FILE_STATES)?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            FileState {
                file_id: r.get(1)?,
                offset: r.get(2)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Lo nuevo de un archivo, ya parseado y listo para escribir.
#[derive(Debug, Default)]
pub struct ParsedFile {
    key: String,
    /// `None` en las bases SQLite (su identidad no se guarda).
    file_id: Option<String>,
    size: i64,
    offset: i64,
    mtime: i64,
    records: Vec<Record>,
    lines: usize,
    bad_lines: usize,
}

/// Lee y parsea lo nuevo de `path` desde su estado anterior. `None` si no cambió (sin abrirlo).
#[tracing::instrument(name = "ingest_file", level = "debug", skip_all, fields(ruta = %path.display(), bytes = Empty, lines = Empty, records = Empty))]
pub fn read_file(
    provider: &dyn Provider,
    path: &Path,
    prev: Option<&FileState>,
) -> Result<Option<ParsedFile>> {
    let parsed = if provider.source() == Source::Sqlite {
        read_db(provider, path, prev)?
    } else {
        match read_jsonl(provider, path, prev)? {
            Some(p) => p,
            None => return Ok(None),
        }
    };
    let span = tracing::Span::current();
    span.record("lines", parsed.lines);
    span.record("records", parsed.records.len());
    Ok(Some(parsed))
}

fn read_jsonl(
    provider: &dyn Provider,
    path: &Path,
    prev: Option<&FileState>,
) -> Result<Option<ParsedFile>> {
    let meta = fs::metadata(path)?;
    let size = meta.len() as i64;
    let file_id = file_identity(&meta);
    let same_file = |s: &FileState| s.file_id == file_id && size >= s.offset;
    if prev.is_some_and(|s| same_file(s) && s.offset == size) {
        return Ok(None);
    }
    // Un documento completo (no JSONL) se relee entero cada vez que cambia.
    let whole = provider.whole_file(path);
    let start = match prev {
        // A mitad de archivo solo si el proveedor conserva su estado; si no, se relee entero
        // (idempotente: llamadas, herramientas, turnos y eventos tienen id propio).
        Some(s) if !whole && same_file(s) && provider.knows(path) => s.offset,
        _ => 0,
    };
    if start == 0 {
        provider.reset(path);
    }

    let mut parsed = ParsedFile {
        key: path.to_string_lossy().to_string(),
        file_id: Some(file_id),
        size,
        mtime: mtime_ms(&meta),
        ..Default::default()
    };
    let feed = |line: &str, parsed: &mut ParsedFile| {
        parsed.lines += 1;
        match provider.parse_line(path, line) {
            Ok(records) => parsed.records.extend(records),
            Err(_) => parsed.bad_lines += 1,
        }
    };
    let mut reader = BufReader::with_capacity(READ_BUFFER, File::open(path)?);
    reader.seek(SeekFrom::Start(start as u64))?;
    let bytes;
    if whole {
        // Documento completo: todo el archivo es un único registro (aunque tenga saltos y no
        // acabe en salto de línea).
        let mut buf = Vec::new();
        bytes = reader.read_to_end(&mut buf)?;
        let text = String::from_utf8_lossy(&buf);
        let text = text.trim();
        if !text.is_empty() {
            feed(text, &mut parsed);
        }
        // Un documento que aún no parsea (escritura a medias) se reintenta desde 0.
        parsed.offset = if parsed.bad_lines > 0 {
            0
        } else {
            bytes as i64
        };
    } else {
        // Solo líneas completas; lo que quede tras el último \n espera a la siguiente pasada.
        let mut line = Vec::new();
        let mut consumed = 0;
        let mut read = 0;
        loop {
            line.clear();
            let n = reader.read_until(b'\n', &mut line)?;
            read += n;
            if n == 0 || line.last() != Some(&b'\n') {
                break;
            }
            consumed += n;
            let text = String::from_utf8_lossy(&line[..n - 1]);
            let text = text.trim();
            if !text.is_empty() {
                feed(text, &mut parsed);
            }
        }
        bytes = read;
        parsed.offset = start + consumed as i64;
    }
    tracing::Span::current().record("bytes", bytes);
    Ok(Some(parsed))
}

/// Base SQLite de un agente: el "offset" de `file_state` es el cursor de `time_updated`.
fn read_db(provider: &dyn Provider, path: &Path, prev: Option<&FileState>) -> Result<ParsedFile> {
    let meta = fs::metadata(path)?;
    let (records, cursor) = provider.read_db(path, prev.map_or(0, |s| s.offset))?;
    Ok(ParsedFile {
        key: path.to_string_lossy().to_string(),
        file_id: None,
        size: meta.len() as i64,
        offset: cursor,
        mtime: mtime_ms(&meta),
        lines: records.len(),
        records,
        bad_lines: 0,
    })
}

/// Escribe un archivo parseado y su nuevo estado (dentro de la transacción del llamador).
#[tracing::instrument(name = "ingest.tx", level = "debug", skip_all, fields(records = Empty))]
pub fn write_file(
    conn: &Connection,
    provider: &dyn Provider,
    parsed: &ParsedFile,
) -> Result<FileResult> {
    ensure_agent(conn, provider)?;
    let mut result = FileResult {
        lines: parsed.lines,
        bad_lines: parsed.bad_lines,
        ..Default::default()
    };
    let mut w = Writer::new(conn, provider.id());
    for rec in &parsed.records {
        if let Some(id) = w.apply(rec)? {
            result.new_tool_calls.push(id);
        }
        result.records += 1;
    }
    w.backfill_turns()?;
    let now = now_ms();
    match &parsed.file_id {
        Some(file_id) => conn.prepare_cached(UPSERT_FILE_STATE)?.execute(params![
            parsed.key,
            provider.id(),
            file_id,
            parsed.size,
            parsed.offset,
            parsed.mtime,
            now
        ])?,
        None => conn.prepare_cached(UPSERT_DB_STATE)?.execute(params![
            parsed.key,
            provider.id(),
            parsed.size,
            parsed.offset,
            parsed.mtime,
            now
        ])?,
    };
    tracing::Span::current().record("records", result.records);
    Ok(result)
}

/// Lee lo nuevo de un archivo y lo guarda en una transacción.
pub fn ingest_file(
    conn: &mut Connection,
    provider: &dyn Provider,
    path: &Path,
) -> Result<FileResult> {
    let prev = conn
        .prepare_cached(SELECT_FILE_STATE)?
        .query_row([path.to_string_lossy()], |r| {
            Ok(FileState {
                file_id: r.get(0)?,
                offset: r.get(1)?,
            })
        })
        .optional()?;
    ingest_file_from(conn, provider, path, prev.as_ref())
}

fn ingest_file_from(
    conn: &mut Connection,
    provider: &dyn Provider,
    path: &Path,
    prev: Option<&FileState>,
) -> Result<FileResult> {
    let Some(parsed) = read_file(provider, path, prev)? else {
        return Ok(FileResult::default());
    };
    let tx = conn.transaction()?;
    let result = write_file(&tx, provider, &parsed)?;
    tx.commit()?;
    Ok(result)
}

const READ_BUFFER: usize = 64 * 1024;

const UPSERT_FILE_STATE: &str =
    "INSERT INTO file_state (path, agent_id, file_id, size, offset, mtime, last_scan)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
     ON CONFLICT(path) DO UPDATE SET file_id = excluded.file_id, size = excluded.size,
       offset = excluded.offset, mtime = excluded.mtime, last_scan = excluded.last_scan";

const UPSERT_DB_STATE: &str =
    "INSERT INTO file_state (path, agent_id, file_id, size, offset, mtime, last_scan)
     VALUES (?1, ?2, 'sqlite', ?3, ?4, ?5, ?6)
     ON CONFLICT(path) DO UPDATE SET size = excluded.size, offset = excluded.offset,
       mtime = excluded.mtime, last_scan = excluded.last_scan";

const INSERT_PROJECT: &str =
    "INSERT INTO projects (name, cwd, repo_root) VALUES (?1, ?2, ?3) ON CONFLICT(cwd) DO NOTHING";

const SELECT_PROJECT: &str = "SELECT id FROM projects WHERE cwd = ?1";

const BACKFILL_CALLS: &str =
    "UPDATE calls SET turn_id = (SELECT id FROM turns t WHERE t.session_id = calls.session_id
       AND t.ts <= calls.ts ORDER BY t.ts DESC LIMIT 1)
     WHERE session_id = ?1 AND turn_id IS NULL AND ts >= ?2";

const BACKFILL_TOOL_CALLS: &str = "UPDATE tool_calls SET turn_id = (SELECT id FROM turns t WHERE t.session_id = tool_calls.session_id
       AND t.ts <= tool_calls.ts ORDER BY t.ts DESC LIMIT 1)
     WHERE session_id = ?1 AND turn_id IS NULL AND ts >= ?2";

const UPSERT_SESSION: &str =
    "INSERT INTO sessions (id, agent_id, project_id, git_branch, started_at, ended_at, is_subagent)
     VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)
     ON CONFLICT(id) DO UPDATE SET
       started_at = MIN(started_at, excluded.started_at),
       ended_at   = MAX(ended_at, excluded.ended_at),
       git_branch = COALESCE(excluded.git_branch, git_branch),
       project_id = COALESCE(project_id, excluded.project_id),
       is_subagent = MAX(is_subagent, excluded.is_subagent)";

const UPSERT_CALL: &str = "INSERT INTO calls (message_id, session_id, ts, model, input_tokens, output_tokens,
        cache_read, cache_write, cache_write_1h, reasoning_tokens, activity, is_sidechain, agent_id, cost_reported, turn_id)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, (SELECT id FROM turns
       WHERE session_id = ?2 AND ts <= ?3 ORDER BY ts DESC LIMIT 1))
     ON CONFLICT(message_id) DO UPDATE SET
       ts = MIN(ts, excluded.ts), model = excluded.model,
       input_tokens = excluded.input_tokens, output_tokens = excluded.output_tokens,
       cache_read = excluded.cache_read, cache_write = excluded.cache_write,
       cache_write_1h = excluded.cache_write_1h, reasoning_tokens = excluded.reasoning_tokens,
       activity = COALESCE(excluded.activity, activity),
       is_sidechain = excluded.is_sidechain, agent_id = COALESCE(excluded.agent_id, agent_id),
       cost_reported = COALESCE(excluded.cost_reported, cost_reported),
       turn_id = COALESCE(turn_id, excluded.turn_id)";

const UPDATE_SESSION_MODEL: &str = "UPDATE sessions SET model = ?1 WHERE id = ?2";

const INSERT_TOOL_USE: &str =
    "INSERT INTO tool_calls (call_id, message_id, session_id, ts, tool, target, detail, turn_id)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, (SELECT id FROM turns
       WHERE session_id = ?3 AND ts <= ?4 ORDER BY ts DESC LIMIT 1))
     ON CONFLICT(call_id) DO NOTHING";

const REFRESH_TOOL_USE: &str = "UPDATE tool_calls SET detail = COALESCE(detail, ?1),
       turn_id = COALESCE(turn_id, (SELECT id FROM turns WHERE session_id = ?2 AND ts <= ?3 ORDER BY ts DESC LIMIT 1))
     WHERE call_id = ?4";

const UPDATE_TOOL_RESULT: &str = "UPDATE tool_calls SET is_error = ?1, duration_ms = MAX(0, ?2 - ts), agent_id = COALESCE(?3, agent_id)
     WHERE call_id = ?4";

const UPSERT_TURN: &str = "INSERT INTO turns (id, session_id, ts, intent) VALUES (?1, ?2, ?3, ?4)
     ON CONFLICT(id) DO UPDATE SET ts = MIN(ts, excluded.ts), intent = COALESCE(intent, excluded.intent)
     RETURNING ts";

const INSERT_EVENT: &str =
    "INSERT OR IGNORE INTO events (session_id, ts, kind, payload_json) VALUES (?1, ?2, ?3, ?4)";

/// Aplica registros a la base dentro de una transacción.
struct Writer<'a> {
    tx: &'a Connection,
    agent_id: &'static str,
    projects: HashMap<String, i64>,
    /// Por sesión, el `ts` mínimo de los turnos aplicados: solo las filas sin turno desde ahí
    /// pueden ganar uno.
    new_turns: HashMap<String, i64>,
}

impl<'a> Writer<'a> {
    fn new(tx: &'a Connection, agent_id: &'static str) -> Self {
        Self {
            tx,
            agent_id,
            projects: HashMap::new(),
            new_turns: HashMap::new(),
        }
    }

    fn run(&self, sql: &str, params: impl rusqlite::Params) -> Result<usize> {
        Ok(self.tx.prepare_cached(sql)?.execute(params)?)
    }

    fn project_id(&mut self, cwd: &str) -> Result<i64> {
        if let Some(id) = self.projects.get(cwd) {
            return Ok(*id);
        }
        let (name, repo_root) = project_of(cwd);
        self.run(INSERT_PROJECT, params![name, cwd, repo_root])?;
        let id: i64 = self
            .tx
            .prepare_cached(SELECT_PROJECT)?
            .query_row([cwd], |r| r.get(0))?;
        self.projects.insert(cwd.to_string(), id);
        Ok(id)
    }

    /// Asigna turno a llamadas y herramientas que llegaron antes que él (p. ej. el archivo
    /// de un subagente leído antes que el de su sesión).
    fn backfill_turns(&self) -> Result<()> {
        for (sid, since) in &self.new_turns {
            for sql in [BACKFILL_CALLS, BACKFILL_TOOL_CALLS] {
                self.run(sql, params![sid, since])?;
            }
        }
        Ok(())
    }

    /// Devuelve el `call_id` si se insertó una tool call nueva.
    fn apply(&mut self, rec: &Record) -> Result<Option<String>> {
        match rec {
            Record::Session(s) => {
                let project = match &s.cwd {
                    Some(cwd) => Some(self.project_id(cwd)?),
                    None => None,
                };
                self.run(
                    UPSERT_SESSION,
                    params![
                        s.id,
                        self.agent_id,
                        project,
                        s.git_branch,
                        s.ts,
                        s.is_subagent
                    ],
                )?;
            }
            Record::Call(c) => {
                self.run(
                    UPSERT_CALL,
                    params![
                        c.message_id,
                        c.session_id,
                        c.ts,
                        c.model,
                        c.input_tokens,
                        c.output_tokens,
                        c.cache_read,
                        c.cache_write,
                        c.cache_write_1h,
                        c.reasoning_tokens,
                        c.activity,
                        c.is_sidechain,
                        c.agent_id,
                        c.cost_reported
                    ],
                )?;
                if !c.is_sidechain {
                    self.run(UPDATE_SESSION_MODEL, params![c.model, c.session_id])?;
                }
            }
            Record::ToolUse(t) => {
                let n = self.run(
                    INSERT_TOOL_USE,
                    params![
                        t.call_id,
                        t.message_id,
                        t.session_id,
                        t.ts,
                        t.tool,
                        t.target,
                        t.detail
                    ],
                )?;
                if n > 0 {
                    return Ok(Some(t.call_id.clone()));
                }
                // Ya existía (relectura): completa las columnas añadidas después.
                self.run(
                    REFRESH_TOOL_USE,
                    params![t.detail, t.session_id, t.ts, t.call_id],
                )?;
            }
            Record::ToolResult {
                call_id,
                ts,
                is_error,
                agent_id,
            } => {
                self.run(UPDATE_TOOL_RESULT, params![is_error, ts, agent_id, call_id])?;
            }
            Record::Turn(t) => {
                let ts: i64 = self
                    .tx
                    .prepare_cached(UPSERT_TURN)?
                    .query_row(params![t.id, t.session_id, t.ts, t.intent], |r| r.get(0))?;
                self.new_turns
                    .entry(t.session_id.clone())
                    .and_modify(|min| *min = (*min).min(ts))
                    .or_insert(ts);
            }
            Record::Event(e) => {
                self.run(
                    INSERT_EVENT,
                    params![e.session_id, e.ts, e.kind, e.payload_json],
                )?;
            }
        }
        Ok(None)
    }
}

/// Nombre de proyecto y raíz del repo; los worktrees de `.claude/worktrees/` se agrupan bajo su repo.
pub fn project_of(cwd: &str) -> (String, String) {
    let norm = cwd.replace('\\', "/");
    let root = match norm.find("/.claude/worktrees/") {
        Some(i) => &cwd[..i],
        None => cwd,
    };
    let name = root
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(root)
        .to_string();
    (name, root.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::providers::claude_code::ClaudeCode;
    use std::io::Write;

    fn line(msg_id: &str, out: i64, ts: &str) -> String {
        format!(
            r#"{{"type":"assistant","sessionId":"s1","timestamp":"{ts}","cwd":"/h/Proyectos/AgentBoard","message":{{"model":"claude-sonnet-4-5","id":"{msg_id}","content":[],"usage":{{"input_tokens":10,"output_tokens":{out}}}}}}}"#
        )
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn setup() -> (tempfile::TempDir, PathBuf, ClaudeCode, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("projects");
        fs::create_dir_all(root.join("p")).unwrap();
        let file = root.join("p").join("s1.jsonl");
        let provider = ClaudeCode::with_roots(vec![root]);
        (dir, file, provider, db::open_in_memory().unwrap())
    }

    fn tool_line(i: usize) -> String {
        format!(
            r#"{{"type":"assistant","sessionId":"s1","timestamp":"2026-09-25T10:{:02}:{:02}Z","cwd":"/h/Proyectos/AgentBoard","message":{{"model":"claude-sonnet-4-5","id":"m{i}","content":[{{"type":"tool_use","id":"tu{i}","name":"Bash","input":{{"command":"ls"}}}}],"usage":{{"input_tokens":10,"output_tokens":5}}}}}}"#,
            (i / 60) % 60,
            i % 60
        )
    }

    fn compiled(log: &[db::trace::Executed]) -> (usize, usize) {
        let distinct: std::collections::HashSet<&str> =
            log.iter().map(|e| e.sql.as_str()).collect();
        (log.iter().filter(|e| e.compiled).count(), distinct.len())
    }

    #[test]
    fn cada_sentencia_se_compila_como_mucho_una_vez_por_archivo() {
        let (_d, file, p, mut conn) = setup();
        let body: String = (0..200).map(|i| tool_line(i) + "\n").collect();
        fs::write(&file, body).unwrap();
        let log = db::trace::start(&conn);
        let r = ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!(r.new_tool_calls.len(), 200);
        let (compiles, distinct) = compiled(&db::trace::take(log));
        assert!(
            compiles <= distinct,
            "{compiles} compilaciones para {distinct} sentencias distintas"
        );
    }

    fn prompt_line(prompt_id: &str, ts: &str) -> String {
        format!(
            r#"{{"type":"user","sessionId":"s1","promptId":"{prompt_id}","timestamp":"{ts}","cwd":"/h/Proyectos/AgentBoard","message":{{"role":"user","content":"hola"}}}}"#
        )
    }

    fn turn_of(conn: &Connection, msg_id: &str) -> Option<String> {
        conn.query_row(
            "SELECT turn_id FROM calls WHERE message_id = ?1",
            [msg_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn releer_sin_turnos_nuevos_no_rellena_turnos() {
        let (_d, file, p, mut conn) = setup();
        let body: String = (0..5).map(|i| tool_line(i) + "\n").collect();
        fs::write(&file, body).unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
        for i in 5..10 {
            writeln!(f, "{}", tool_line(i)).unwrap();
        }
        let log = db::trace::start(&conn);
        assert_eq!(ingest_file(&mut conn, &p, &file).unwrap().lines, 5);
        let backfills: Vec<_> = db::trace::take(log)
            .into_iter()
            .filter(|e| e.sql.contains("SET turn_id"))
            .collect();
        assert!(backfills.is_empty(), "{backfills:?}");
    }

    #[test]
    fn subagente_leido_antes_que_su_turno_queda_asignado() {
        let (_d, file, p, mut conn) = setup();
        let dir = file.parent().unwrap();
        let side = line("ms", 1, "2026-09-25T10:05:00Z").replace(
            r#""type":"assistant","#,
            r#""type":"assistant","isSidechain":true,"agentId":"a1","#,
        );
        fs::write(dir.join("a-subagente.jsonl"), side + "\n").unwrap();
        fs::write(
            dir.join("b-sesion.jsonl"),
            prompt_line("p1", "2026-09-25T10:00:00Z") + "\n",
        )
        .unwrap();
        scan_all(
            &mut conn,
            &[Box::new(ClaudeCode::with_roots(p.log_roots()))],
        )
        .unwrap();
        assert_eq!(turn_of(&conn, "ms").as_deref(), Some("p1"));
    }

    #[test]
    fn un_turno_nuevo_solo_rellena_las_filas_posteriores() {
        let (_d, file, p, mut conn) = setup();
        let body = [
            line("antes", 1, "2026-09-25T09:00:00Z"),
            line("despues", 1, "2026-09-25T10:05:00Z"),
        ]
        .join("\n")
            + "\n";
        fs::write(&file, body).unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
        writeln!(f, "{}", prompt_line("p1", "2026-09-25T10:00:00Z")).unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!(turn_of(&conn, "despues").as_deref(), Some("p1"));
        assert_eq!(turn_of(&conn, "antes"), None);
    }

    #[test]
    fn archivo_rotado_se_relee_desde_cero_sin_duplicar() {
        let (_d, file, p, mut conn) = setup();
        let lines = |ids: &[&str]| -> String {
            ids.iter()
                .map(|id| line(id, 1, "2026-09-25T10:00:00Z") + "\n")
                .collect()
        };
        fs::write(&file, lines(&["m1", "m2"])).unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        let rotated = file.with_extension("tmp");
        fs::write(&rotated, lines(&["m1", "m2", "m3"])).unwrap();
        fs::rename(&rotated, &file).unwrap();
        let r = ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!(r.lines, 3, "otra identidad: se relee desde el byte 0");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM calls"), 3);
    }

    #[cfg(unix)]
    #[test]
    fn archivo_sin_cambios_no_se_abre() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, file, p, mut conn) = setup();
        fs::write(&file, line("m1", 1, "2026-09-25T10:00:00Z") + "\n").unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
        if File::open(&file).is_ok() {
            return; // root ignora los permisos
        }
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(p)];
        let stats = scan_all(&mut conn, &providers).unwrap();
        assert_eq!((stats.files, stats.errors, stats.lines), (1, 0, 0));
    }

    #[test]
    fn el_estado_de_los_archivos_se_carga_una_vez_por_escaneo() {
        let (_d, file, p, mut conn) = setup();
        let dir = file.parent().unwrap();
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            let id = format!("m{i}");
            fs::write(
                dir.join(format!("{name}.jsonl")),
                line(&id, 1, "2026-09-25T10:00:00Z") + "\n",
            )
            .unwrap();
        }
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(p)];
        let log = db::trace::start(&conn);
        assert_eq!(scan_all(&mut conn, &providers).unwrap().files, 3);
        let selects = db::trace::take(log)
            .into_iter()
            .filter(|e| {
                e.sql.trim_start().starts_with("SELECT") && e.sql.contains("FROM file_state")
            })
            .count();
        assert_eq!(selects, 1);
    }

    #[test]
    fn streaming_deja_una_llamada_con_los_ultimos_tokens() {
        let (_d, file, p, mut conn) = setup();
        let body = [
            line("m1", 10, "2026-09-25T10:00:00Z"),
            line("m1", 50, "2026-09-25T10:00:01Z"),
            line("m1", 138, "2026-09-25T10:00:02Z"),
        ]
        .join("\n")
            + "\n";
        fs::write(&file, body).unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM calls"), 1);
        assert_eq!(count(&conn, "SELECT output_tokens FROM calls"), 138);
    }

    #[test]
    fn reimportar_no_duplica_y_solo_lee_lo_nuevo() {
        let (_d, file, p, mut conn) = setup();
        fs::write(&file, line("m1", 1, "2026-09-25T10:00:00Z") + "\n").unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        let again = ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!(again.lines, 0, "no hay nada nuevo que leer");

        let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
        writeln!(f, "{}", line("m2", 1, "2026-09-25T10:01:00Z")).unwrap();
        let r = ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!(r.lines, 1, "solo la línea añadida");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM calls"), 2);
    }

    #[test]
    fn linea_incompleta_espera_a_la_siguiente_pasada() {
        let (_d, file, p, mut conn) = setup();
        let full = line("m1", 1, "2026-09-25T10:00:00Z");
        let (a, b) = full.split_at(40);
        fs::write(&file, a).unwrap();
        assert_eq!(ingest_file(&mut conn, &p, &file).unwrap().lines, 0);
        let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
        writeln!(f, "{b}").unwrap();
        assert_eq!(ingest_file(&mut conn, &p, &file).unwrap().lines, 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM calls"), 1);
    }

    #[test]
    fn archivo_truncado_se_relee_sin_duplicar() {
        let (_d, file, p, mut conn) = setup();
        let two = [
            line("m1", 1, "2026-09-25T10:00:00Z"),
            line("m2", 1, "2026-09-25T10:01:00Z"),
        ]
        .join("\n")
            + "\n";
        fs::write(&file, two).unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        fs::write(&file, line("m1", 1, "2026-09-25T10:00:00Z") + "\n").unwrap();
        let r = ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!(r.lines, 1, "se releyó desde el byte 0");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM calls"), 2);
    }

    #[test]
    fn borrar_el_original_conserva_las_filas() {
        let (_d, file, p, mut conn) = setup();
        fs::write(&file, line("m1", 1, "2026-09-25T10:00:00Z") + "\n").unwrap();
        scan_all(
            &mut conn,
            &[Box::new(ClaudeCode::with_roots(p.log_roots()))],
        )
        .unwrap();
        fs::remove_file(&file).unwrap();
        scan_all(
            &mut conn,
            &[Box::new(ClaudeCode::with_roots(p.log_roots()))],
        )
        .unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM calls"), 1);
    }

    #[test]
    fn sesiones_de_la_misma_carpeta_comparten_proyecto() {
        let (_d, file, p, mut conn) = setup();
        let other = line("m2", 1, "2026-09-25T10:00:00Z").replace("\"s1\"", "\"s2\"");
        fs::write(
            &file,
            line("m1", 1, "2026-09-25T10:00:00Z") + "\n" + &other + "\n",
        )
        .unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM sessions"), 2);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM projects"), 1);
        let name: String = conn
            .query_row("SELECT name FROM projects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "AgentBoard");
    }

    #[test]
    fn nombre_de_proyecto_y_worktrees() {
        assert_eq!(project_of("/h/Proyectos/AgentBoard").0, "AgentBoard");
        assert_eq!(
            project_of("/h/repo/.claude/worktrees/feat-x"),
            ("repo".into(), "/h/repo".into())
        );
        assert_eq!(project_of("C:\\Users\\r\\code\\app").0, "app");
    }

    #[test]
    fn documento_json_completo_se_lee_y_relee() {
        use crate::providers::gemini::Gemini;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("tmp");
        fs::create_dir_all(root.join("h").join("chats")).unwrap();
        let file = root.join("h").join("chats").join("antigua.json");
        let p = Gemini::with_roots(vec![root]);
        let mut conn = db::open_in_memory().unwrap();
        let msg = |id: &str, out: i64| {
            format!(
                r#"{{"id":"{id}","timestamp":"2026-09-24T11:00:05.000Z","type":"gemini","content":"ok","model":"gemini-2.5-pro","tokens":{{"input":100,"output":{out},"cached":0,"total":100}}}}"#
            )
        };
        // Compacto y sin salto de línea final: antes la ingesta no entregaba nada.
        let doc = format!(r#"{{"sessionId":"g-old","messages":[{}]}}"#, msg("m1", 10));
        fs::write(&file, &doc).unwrap();
        let r = ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!((r.lines, r.bad_lines), (1, 0));
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM calls"), 1);
        let again = ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!(again.lines, 0, "sin cambios no se relee");
        // Reescrito con un mensaje más (sigue sin salto final): se relee entero sin duplicar.
        let doc2 = format!(
            r#"{{"sessionId":"g-old","messages":[{},{}]}}"#,
            msg("m1", 10),
            msg("m2", 20)
        );
        fs::write(&file, &doc2).unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM calls"), 2);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM sessions"), 1);
    }

    #[test]
    fn documento_json_formateado_en_varias_lineas() {
        use crate::providers::gemini::Gemini;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("tmp");
        fs::create_dir_all(root.join("h").join("chats")).unwrap();
        let file = root.join("h").join("chats").join("antigua.json");
        let p = Gemini::with_roots(vec![root]);
        let mut conn = db::open_in_memory().unwrap();
        let doc = "{\n  \"sessionId\": \"g-old\",\n  \"messages\": [\n    {\"id\": \"m1\", \"timestamp\": \"2026-09-24T11:00:05.000Z\", \"type\": \"gemini\", \"content\": \"ok\", \"model\": \"gemini-2.5-pro\", \"tokens\": {\"input\": 100, \"output\": 10, \"cached\": 0, \"total\": 110}}\n  ]\n}\n";
        fs::write(&file, doc).unwrap();
        let r = ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!(
            (r.lines, r.bad_lines),
            (1, 0),
            "el documento se parsea entero"
        );
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM calls"), 1);
    }

    #[test]
    fn no_guarda_texto_de_mensajes() {
        let (_d, file, p, mut conn) = setup();
        let l = line("m1", 1, "2026-09-25T10:00:00Z").replace(
            "\"content\":[]",
            "\"content\":[{\"type\":\"text\",\"text\":\"SECRETO\"}]",
        );
        fs::write(&file, l + "\n").unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        let dump: String = conn
            .query_row(
                "SELECT group_concat(quote(message_id)||quote(activity)) FROM calls",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!dump.contains("SECRETO"));
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM events"), 0);
    }

    #[cfg(unix)]
    #[test]
    fn archivo_ilegible_avisa_con_la_ruta_sin_configurar_nada() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, file, p, mut conn) = setup();
        fs::write(&file, line("m1", 1, "2026-09-25T10:00:00Z") + "\n").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
        if File::open(&file).is_ok() {
            return; // root ignora los permisos: no hay forma de provocar el error
        }
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(p)];
        let mut stats = ScanStats::default();
        let out = crate::telemetry::tests::captured(None, || {
            stats = scan_all(&mut conn, &providers).unwrap();
        });
        assert_eq!(stats.errors, 1);
        assert!(out.contains("WARN"), "{out}");
        assert!(out.contains(&file.display().to_string()), "{out}");
        assert!(out.contains("error leyendo"), "{out}");
    }

    #[test]
    fn la_ingesta_emite_spans_de_escaneo_recorrido_archivo_e_insercion() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/claude_code");
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(ClaudeCode::with_roots(vec![root]))];
        let mut conn = db::open_in_memory().unwrap();
        let spans = crate::telemetry::tests::closed_spans(|| {
            scan_all(&mut conn, &providers).unwrap();
        });
        let named = |n: &str| spans.iter().filter(|s| s.name == n).collect::<Vec<_>>();
        let scan = named("scan_all");
        assert_eq!(scan.len(), 1, "{spans:?}");
        assert_eq!(scan[0].num("files"), 2);
        assert!(scan[0].num("records") > 0);
        assert_eq!(scan[0].num("errors"), 0);
        assert_eq!(named("walk").len(), 1, "{spans:?}");
        assert_eq!(named("walk")[0].num("files"), 2);
        let files = named("ingest_file");
        assert_eq!(files.len(), 2, "{spans:?}");
        for f in &files {
            assert!(f.fields.contains_key("ruta"), "{f:?}");
            assert!(
                f.num("bytes") > 0 && f.num("lines") > 0 && f.num("records") > 0,
                "{f:?}"
            );
        }
        assert_eq!(named("ingest.tx").len(), 2, "{spans:?}");
    }
}
