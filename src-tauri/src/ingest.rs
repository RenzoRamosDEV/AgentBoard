//! Lectura incremental de logs: offset por archivo, upsert por id y nada se borra solo.

use crate::providers::{Provider, Record};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

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

/// Registra cada agente cuya carpeta de logs exista.
pub fn register_agents(conn: &Connection, providers: &[Box<dyn Provider>]) -> Result<()> {
    for p in providers {
        if let Some(root) = p.log_roots().into_iter().find(|r| r.is_dir()) {
            conn.execute(
                "INSERT INTO agents (id, name, log_root, first_seen) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET log_root = excluded.log_root",
                params![p.id(), p.name(), root.to_string_lossy(), now_ms()],
            )?;
        }
    }
    Ok(())
}

/// Garantiza la fila del agente (las sesiones la referencian).
fn ensure_agent(conn: &Connection, p: &dyn Provider) -> Result<()> {
    let root = p.log_roots().first().map(|r| r.to_string_lossy().to_string()).unwrap_or_default();
    conn.execute(
        "INSERT OR IGNORE INTO agents (id, name, log_root, first_seen) VALUES (?1, ?2, ?3, ?4)",
        params![p.id(), p.name(), root, now_ms()],
    )?;
    Ok(())
}

/// Escaneo completo: importa todo lo que haya en disco (solo lo nuevo si ya se importó).
pub fn scan_all(conn: &mut Connection, providers: &[Box<dyn Provider>]) -> Result<ScanStats> {
    register_agents(conn, providers)?;
    let mut stats = ScanStats::default();
    for p in providers {
        for root in p.log_roots().into_iter().filter(|r| r.is_dir()) {
            for path in walk(&root) {
                if !p.matches(&path) {
                    continue;
                }
                match ingest_file(conn, p.as_ref(), &path) {
                    Ok(r) => {
                        stats.files += 1;
                        stats.lines += r.lines;
                        stats.records += r.records;
                    }
                    Err(e) => {
                        stats.errors += 1;
                        eprintln!("agentboard: error leyendo {}: {e:#}", path.display());
                    }
                }
            }
        }
    }
    Ok(stats)
}

/// Archivos bajo `root`, recursivo, sin seguir enlaces.
pub fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => out.push(e.path()),
                _ => {}
            }
        }
    }
    out.sort();
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

/// Lee lo nuevo de un archivo y lo guarda en una transacción.
pub fn ingest_file(conn: &mut Connection, provider: &dyn Provider, path: &Path) -> Result<FileResult> {
    let meta = fs::metadata(path)?;
    let size = meta.len() as i64;
    let file_id = file_identity(&meta);
    let key = path.to_string_lossy().to_string();

    let prev: Option<(String, i64)> = conn
        .query_row("SELECT file_id, offset FROM file_state WHERE path = ?1", [&key], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    let start = match &prev {
        Some((fid, off)) if *fid == file_id && size >= *off => *off,
        _ => 0,
    };
    if prev.is_some() && start == size {
        return Ok(FileResult::default());
    }

    let mut buf = Vec::with_capacity((size - start).max(0) as usize);
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(start as u64))?;
    f.read_to_end(&mut buf)?;

    // Solo líneas completas; lo que quede tras el último \n espera a la siguiente pasada.
    let consumed = buf.iter().rposition(|b| *b == b'\n').map(|i| i + 1).unwrap_or(0);
    let mut result = FileResult::default();
    let tx = conn.transaction()?;
    ensure_agent(&tx, provider)?;
    {
        let mut w = Writer::new(&tx, provider.id());
        for raw in buf[..consumed].split(|b| *b == b'\n') {
            let line = String::from_utf8_lossy(raw);
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            result.lines += 1;
            match provider.parse_line(path, line) {
                Ok(records) => {
                    for rec in records {
                        if let Some(id) = w.apply(&rec)? {
                            result.new_tool_calls.push(id);
                        }
                        result.records += 1;
                    }
                }
                Err(_) => result.bad_lines += 1,
            }
        }
        w.backfill_turns()?;
    }
    tx.execute(
        "INSERT INTO file_state (path, agent_id, file_id, size, offset, mtime, last_scan)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(path) DO UPDATE SET file_id = excluded.file_id, size = excluded.size,
           offset = excluded.offset, mtime = excluded.mtime, last_scan = excluded.last_scan",
        params![key, provider.id(), file_id, size, start + consumed as i64, mtime_ms(&meta), now_ms()],
    )?;
    tx.commit()?;
    Ok(result)
}

/// Aplica registros a la base dentro de una transacción.
struct Writer<'a> {
    tx: &'a Transaction<'a>,
    agent_id: &'static str,
    projects: HashMap<String, i64>,
    sessions: std::collections::HashSet<String>,
}

impl<'a> Writer<'a> {
    fn new(tx: &'a Transaction<'a>, agent_id: &'static str) -> Self {
        Self { tx, agent_id, projects: HashMap::new(), sessions: Default::default() }
    }

    fn project_id(&mut self, cwd: &str) -> Result<i64> {
        if let Some(id) = self.projects.get(cwd) {
            return Ok(*id);
        }
        let (name, repo_root) = project_of(cwd);
        self.tx.execute(
            "INSERT INTO projects (name, cwd, repo_root) VALUES (?1, ?2, ?3) ON CONFLICT(cwd) DO NOTHING",
            params![name, cwd, repo_root],
        )?;
        let id: i64 = self.tx.query_row("SELECT id FROM projects WHERE cwd = ?1", [cwd], |r| r.get(0))?;
        self.projects.insert(cwd.to_string(), id);
        Ok(id)
    }

    /// Asigna turno a llamadas y herramientas que llegaron antes que él (p. ej. el archivo
    /// de un subagente leído antes que el de su sesión).
    fn backfill_turns(&self) -> Result<()> {
        for sid in &self.sessions {
            for table in ["calls", "tool_calls"] {
                self.tx.execute(
                    &format!(
                        "UPDATE {table} SET turn_id = (SELECT id FROM turns t WHERE t.session_id = {table}.session_id
                           AND t.ts <= {table}.ts ORDER BY t.ts DESC LIMIT 1)
                         WHERE session_id = ?1 AND turn_id IS NULL"
                    ),
                    [sid],
                )?;
            }
        }
        Ok(())
    }

    /// Devuelve el `call_id` si se insertó una tool call nueva.
    fn apply(&mut self, rec: &Record) -> Result<Option<String>> {
        match rec {
            Record::Session(s) => {
                if !self.sessions.contains(&s.id) {
                    self.sessions.insert(s.id.clone());
                }
                let project = match &s.cwd {
                    Some(cwd) => Some(self.project_id(cwd)?),
                    None => None,
                };
                self.tx.execute(
                    "INSERT INTO sessions (id, agent_id, project_id, git_branch, started_at, ended_at, is_subagent)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)
                     ON CONFLICT(id) DO UPDATE SET
                       started_at = MIN(started_at, excluded.started_at),
                       ended_at   = MAX(ended_at, excluded.ended_at),
                       git_branch = COALESCE(excluded.git_branch, git_branch),
                       project_id = COALESCE(project_id, excluded.project_id)",
                    params![s.id, self.agent_id, project, s.git_branch, s.ts, s.is_subagent],
                )?;
            }
            Record::Call(c) => {
                self.tx.execute(
                    "INSERT INTO calls (message_id, session_id, ts, model, input_tokens, output_tokens,
                        cache_read, cache_write, cache_write_1h, reasoning_tokens, activity, is_sidechain, agent_id, turn_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, (SELECT id FROM turns
                       WHERE session_id = ?2 AND ts <= ?3 ORDER BY ts DESC LIMIT 1))
                     ON CONFLICT(message_id) DO UPDATE SET
                       ts = MIN(ts, excluded.ts), model = excluded.model,
                       input_tokens = excluded.input_tokens, output_tokens = excluded.output_tokens,
                       cache_read = excluded.cache_read, cache_write = excluded.cache_write,
                       cache_write_1h = excluded.cache_write_1h, reasoning_tokens = excluded.reasoning_tokens,
                       activity = COALESCE(excluded.activity, activity),
                       is_sidechain = excluded.is_sidechain, agent_id = COALESCE(excluded.agent_id, agent_id),
                       turn_id = COALESCE(turn_id, excluded.turn_id)",
                    params![
                        c.message_id, c.session_id, c.ts, c.model, c.input_tokens, c.output_tokens,
                        c.cache_read, c.cache_write, c.cache_write_1h, c.reasoning_tokens, c.activity,
                        c.is_sidechain, c.agent_id
                    ],
                )?;
                if !c.is_sidechain {
                    self.tx.execute("UPDATE sessions SET model = ?1 WHERE id = ?2", params![c.model, c.session_id])?;
                }
            }
            Record::ToolUse(t) => {
                let n = self.tx.execute(
                    "INSERT INTO tool_calls (call_id, message_id, session_id, ts, tool, target, detail, turn_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, (SELECT id FROM turns
                       WHERE session_id = ?3 AND ts <= ?4 ORDER BY ts DESC LIMIT 1))
                     ON CONFLICT(call_id) DO NOTHING",
                    params![t.call_id, t.message_id, t.session_id, t.ts, t.tool, t.target, t.detail],
                )?;
                if n > 0 {
                    return Ok(Some(t.call_id.clone()));
                }
                // Ya existía (relectura): completa las columnas añadidas después.
                self.tx.execute(
                    "UPDATE tool_calls SET detail = COALESCE(detail, ?1),
                       turn_id = COALESCE(turn_id, (SELECT id FROM turns WHERE session_id = ?2 AND ts <= ?3 ORDER BY ts DESC LIMIT 1))
                     WHERE call_id = ?4",
                    params![t.detail, t.session_id, t.ts, t.call_id],
                )?;
            }
            Record::ToolResult { call_id, ts, is_error, agent_id } => {
                self.tx.execute(
                    "UPDATE tool_calls SET is_error = ?1, duration_ms = MAX(0, ?2 - ts), agent_id = COALESCE(?3, agent_id)
                     WHERE call_id = ?4",
                    params![is_error, ts, agent_id, call_id],
                )?;
            }
            Record::Turn(t) => {
                self.tx.execute(
                    "INSERT INTO turns (id, session_id, ts, intent) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(id) DO UPDATE SET ts = MIN(ts, excluded.ts), intent = COALESCE(intent, excluded.intent)",
                    params![t.id, t.session_id, t.ts, t.intent],
                )?;
            }
            Record::Event(e) => {
                self.tx.execute(
                    "INSERT OR IGNORE INTO events (session_id, ts, kind, payload_json) VALUES (?1, ?2, ?3, ?4)",
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

    #[test]
    fn streaming_deja_una_llamada_con_los_ultimos_tokens() {
        let (_d, file, p, mut conn) = setup();
        let body = [line("m1", 10, "2026-09-25T10:00:00Z"), line("m1", 50, "2026-09-25T10:00:01Z"), line("m1", 138, "2026-09-25T10:00:02Z")].join("\n") + "\n";
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
        let two = [line("m1", 1, "2026-09-25T10:00:00Z"), line("m2", 1, "2026-09-25T10:01:00Z")].join("\n") + "\n";
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
        scan_all(&mut conn, &[Box::new(ClaudeCode::with_roots(p.log_roots()))]).unwrap();
        fs::remove_file(&file).unwrap();
        scan_all(&mut conn, &[Box::new(ClaudeCode::with_roots(p.log_roots()))]).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM calls"), 1);
    }

    #[test]
    fn sesiones_de_la_misma_carpeta_comparten_proyecto() {
        let (_d, file, p, mut conn) = setup();
        let other = line("m2", 1, "2026-09-25T10:00:00Z").replace("\"s1\"", "\"s2\"");
        fs::write(&file, line("m1", 1, "2026-09-25T10:00:00Z") + "\n" + &other + "\n").unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM sessions"), 2);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM projects"), 1);
        let name: String = conn.query_row("SELECT name FROM projects", [], |r| r.get(0)).unwrap();
        assert_eq!(name, "AgentBoard");
    }

    #[test]
    fn nombre_de_proyecto_y_worktrees() {
        assert_eq!(project_of("/h/Proyectos/AgentBoard").0, "AgentBoard");
        assert_eq!(project_of("/h/repo/.claude/worktrees/feat-x"), ("repo".into(), "/h/repo".into()));
        assert_eq!(project_of("C:\\Users\\r\\code\\app").0, "app");
    }

    #[test]
    fn no_guarda_texto_de_mensajes() {
        let (_d, file, p, mut conn) = setup();
        let l = line("m1", 1, "2026-09-25T10:00:00Z").replace("\"content\":[]", "\"content\":[{\"type\":\"text\",\"text\":\"SECRETO\"}]");
        fs::write(&file, l + "\n").unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        let dump: String = conn
            .query_row("SELECT group_concat(quote(message_id)||quote(activity)) FROM calls", [], |r| r.get(0))
            .unwrap();
        assert!(!dump.contains("SECRETO"));
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM events"), 0);
    }
}
