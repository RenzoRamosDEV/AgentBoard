//! Lectura incremental de logs: offset por archivo, upsert por id y nada se borra solo.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use crate::providers::{Provider, Record, Source};
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Condvar, Mutex};
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
pub fn scan_all(conn: &mut Connection, providers: &[Box<dyn Provider>]) -> Result<ScanStats> {
    scan_all_with_threads(conn, providers, default_threads())
}

/// `scan_all` leyendo y parseando con `threads` hilos (el resultado no depende de cuántos).
pub fn scan_all_with_threads(
    conn: &mut Connection,
    providers: &[Box<dyn Provider>],
    threads: usize,
) -> Result<ScanStats> {
    scan(&mut ExclusiveDb(conn), providers, threads)
}

/// `scan_all` sobre una base compartida: el `Mutex` solo se toma para escribir cada lote, así
/// que otras hebras pueden consultar mientras se leen y parsean los archivos.
pub fn scan_shared(db: &Mutex<Connection>, providers: &[Box<dyn Provider>]) -> Result<ScanStats> {
    scan(&mut SharedDb(db), providers, default_threads())
}

pub fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// Registros por transacción del escritor: suelta la base cada pocos milisegundos.
const BATCH_RECORDS: usize = 5_000;
/// Archivos leídos por delante del escritor, por hilo de lectura (acota la memoria en vuelo).
const WINDOW_PER_THREAD: usize = 4;

/// Acceso del escritor a la base: una conexión propia o una compartida tras un `Mutex`.
trait DbSink {
    fn with<T>(&mut self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T>;
}

struct ExclusiveDb<'a>(&'a mut Connection);

impl DbSink for ExclusiveDb<'_> {
    fn with<T>(&mut self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        f(self.0)
    }
}

struct SharedDb<'a>(&'a Mutex<Connection>);

impl DbSink for SharedDb<'_> {
    fn with<T>(&mut self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut conn = self
            .0
            .lock()
            .map_err(|e| anyhow!("base de datos envenenada: {e}"))?;
        f(&mut conn)
    }
}

/// Un archivo que leer, en el orden en que se escribe: proveedor, raíz y ruta ordenada.
struct Job {
    provider: usize,
    path: PathBuf,
}

type ReadResult = Result<Option<ParsedFile>>;

#[tracing::instrument(name = "scan_all", level = "info", skip_all, fields(files = Empty, lines = Empty, records = Empty, errors = Empty))]
fn scan<D: DbSink>(
    db: &mut D,
    providers: &[Box<dyn Provider>],
    threads: usize,
) -> Result<ScanStats> {
    let states = db.with(|conn| {
        register_agents(conn, providers)?;
        // Proyectos creados por un `cwd` pasajero (versiones anteriores) que ninguna sesión usa.
        conn.execute("DELETE FROM projects WHERE id NOT IN (SELECT project_id FROM sessions WHERE project_id IS NOT NULL)", [])?;
        load_file_states(conn)
    })?;
    let mut stats = ScanStats::default();
    let mut jobs = Vec::new();
    for (i, p) in providers.iter().enumerate() {
        for root in p.log_roots().into_iter().filter(|r| r.is_dir()) {
            for path in walk(&root).into_iter().filter(|path| p.matches(path)) {
                // Los archivos sin cambios se descartan aquí, sin abrirlos ni ocupar un lector.
                if p.source() == Source::Jsonl
                    && is_unchanged(&path, states.get(path.to_string_lossy().as_ref()))
                {
                    stats.files += 1;
                } else {
                    jobs.push(Job { provider: i, path });
                }
            }
        }
    }
    let threads = threads.clamp(1, jobs.len().max(1));
    let window = Window::new(WINDOW_PER_THREAD * threads);
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| -> Result<()> {
        let (tx, rx) = mpsc::sync_channel::<(usize, ReadResult)>(window.size);
        let dispatch = tracing::dispatcher::get_default(|d| d.clone());
        let parent = tracing::Span::current();
        for _ in 0..threads {
            let tx = tx.clone();
            let (dispatch, parent) = (dispatch.clone(), parent.clone());
            let (jobs, states, window, next) = (&jobs, &states, &window, &next);
            scope.spawn(move || {
                tracing::dispatcher::with_default(&dispatch, || {
                    let _span = parent.entered();
                    read_jobs(providers, jobs, states, window, next, &tx);
                })
            });
        }
        drop(tx);
        let written = write_in_order(db, providers, &jobs, rx, &window, &mut stats);
        window.close();
        written
    })?;
    let span = tracing::Span::current();
    span.record("files", stats.files);
    span.record("lines", stats.lines);
    span.record("records", stats.records);
    span.record("errors", stats.errors);
    Ok(stats)
}

/// Cuántos archivos ha recogido ya el escritor: un lector no empieza el archivo `i` hasta que
/// `i < recogidos + size`. El siguiente que espera el escritor siempre cabe, así que no hay
/// interbloqueo, y la memoria en vuelo queda acotada.
struct Window {
    size: usize,
    taken: Mutex<(usize, bool)>,
    moved: Condvar,
}

impl Window {
    fn new(size: usize) -> Self {
        Self {
            size,
            taken: Mutex::new((0, false)),
            moved: Condvar::new(),
        }
    }

    /// Espera a que el archivo `i` quepa en la ventana; `false` si el escaneo terminó antes.
    fn wait_for(&self, i: usize) -> bool {
        let mut state = self.taken.lock().unwrap_or_else(|e| e.into_inner());
        while i >= state.0 + self.size && !state.1 {
            state = self.moved.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        !state.1
    }

    fn advance(&self, taken: usize) {
        self.taken.lock().unwrap_or_else(|e| e.into_inner()).0 = taken;
        self.moved.notify_all();
    }

    fn close(&self) {
        self.taken.lock().unwrap_or_else(|e| e.into_inner()).1 = true;
        self.moved.notify_all();
    }
}

fn read_jobs(
    providers: &[Box<dyn Provider>],
    jobs: &[Job],
    states: &HashMap<String, FileState>,
    window: &Window,
    next: &AtomicUsize,
    tx: &mpsc::SyncSender<(usize, ReadResult)>,
) {
    loop {
        let i = next.fetch_add(1, Ordering::Relaxed);
        let Some(job) = jobs.get(i) else {
            return;
        };
        if !window.wait_for(i) {
            return;
        }
        let prev = states.get(job.path.to_string_lossy().as_ref());
        let read = read_file(providers[job.provider].as_ref(), &job.path, prev);
        if tx.send((i, read)).is_err() {
            return;
        }
    }
}

/// Recibe los archivos leídos en cualquier orden y los escribe en el orden de `jobs`, por lotes.
fn write_in_order<D: DbSink>(
    db: &mut D,
    providers: &[Box<dyn Provider>],
    jobs: &[Job],
    rx: mpsc::Receiver<(usize, ReadResult)>,
    window: &Window,
    stats: &mut ScanStats,
) -> Result<()> {
    let mut pending: BTreeMap<usize, ReadResult> = BTreeMap::new();
    let mut batch: Vec<(usize, ParsedFile)> = Vec::new();
    let mut batch_records = 0;
    let mut taken = 0;
    while taken < jobs.len() {
        let Some(read) = pending.remove(&taken) else {
            let received = match rx.try_recv() {
                Ok(r) => r,
                Err(_) => {
                    // Nada listo: se escribe lo acumulado antes de esperar al siguiente archivo.
                    write_batch(db, providers, jobs, &mut batch, stats)?;
                    batch_records = 0;
                    rx.recv()
                        .map_err(|_| anyhow!("los lectores terminaron antes de tiempo"))?
                }
            };
            pending.insert(received.0, received.1);
            continue;
        };
        let i = taken;
        taken += 1;
        window.advance(taken);
        match read {
            Ok(None) => stats.files += 1,
            Ok(Some(parsed)) => {
                batch_records += parsed.records.len();
                batch.push((i, parsed));
                if batch_records >= BATCH_RECORDS {
                    write_batch(db, providers, jobs, &mut batch, stats)?;
                    batch_records = 0;
                }
            }
            Err(e) => read_error(&jobs[i].path, &e, stats),
        }
    }
    write_batch(db, providers, jobs, &mut batch, stats)
}

fn read_error(path: &Path, e: &anyhow::Error, stats: &mut ScanStats) {
    stats.errors += 1;
    tracing::warn!(ruta = %path.display(), "agentboard: error leyendo {}: {e:#}", path.display());
}

/// Una transacción por lote y un `SAVEPOINT` por archivo: un fallo deshace solo ese archivo.
fn write_batch<D: DbSink>(
    db: &mut D,
    providers: &[Box<dyn Provider>],
    jobs: &[Job],
    batch: &mut Vec<(usize, ParsedFile)>,
    stats: &mut ScanStats,
) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    db.with(|conn| {
        let mut tx = conn.transaction()?;
        for (i, parsed) in batch.drain(..) {
            let job = &jobs[i];
            let sp = tx.savepoint()?;
            match write_file(&sp, providers[job.provider].as_ref(), &parsed) {
                Ok(r) => {
                    sp.commit()?;
                    stats.files += 1;
                    stats.lines += r.lines;
                    stats.records += r.records;
                }
                Err(e) => {
                    drop(sp);
                    read_error(&job.path, &e, stats);
                }
            }
        }
        tx.commit()?;
        Ok(())
    })
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

/// Misma identidad y nada tras el offset guardado. Si no se puede consultar, no se da por
/// igual: la lectura dirá el error.
fn is_unchanged(path: &Path, prev: Option<&FileState>) -> bool {
    let Some(prev) = prev else {
        return false;
    };
    fs::metadata(path)
        .is_ok_and(|m| file_identity(&m) == prev.file_id && m.len() as i64 == prev.offset)
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
    if prev.is_some_and(|s| s.file_id == file_id && s.offset == size) {
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
    w.finish()?;
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
    let Some(parsed) = read_file(provider, path, prev.as_ref())? else {
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

/// Lo acumulado de los registros siguientes de una sesión en el mismo archivo (Claude Code
/// emite un registro de sesión por línea): se escribe con un único `UPSERT` al terminar.
const MERGE_SESSION: &str =
    "INSERT INTO sessions (id, agent_id, project_id, git_branch, started_at, ended_at, is_subagent)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
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
    /// Sesiones ya escritas en este archivo, con lo acumulado de sus registros siguientes.
    sessions: HashMap<String, SessionAcc>,
    /// Último modelo (no de subagente) de cada sesión, para un único `UPDATE` al terminar.
    models: HashMap<String, String>,
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
            sessions: HashMap::new(),
            models: HashMap::new(),
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

    /// Escribe lo acumulado de las sesiones y su último modelo.
    fn finish(&mut self) -> Result<()> {
        for (id, acc) in self.sessions.drain().filter(|(_, acc)| acc.merged) {
            self.tx.prepare_cached(MERGE_SESSION)?.execute(params![
                id,
                self.agent_id,
                acc.project,
                acc.branch,
                acc.started,
                acc.ended,
                acc.is_subagent
            ])?;
        }
        for (id, model) in self.models.drain() {
            self.tx
                .prepare_cached(UPDATE_SESSION_MODEL)?
                .execute(params![model, id])?;
        }
        Ok(())
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
                match self.sessions.get_mut(&s.id) {
                    Some(acc) => acc.add(s, project),
                    None => {
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
                        self.sessions.insert(s.id.clone(), SessionAcc::new(s.ts));
                    }
                }
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
                    self.models.insert(c.session_id.clone(), c.model.clone());
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

/// Registros de sesión siguientes al primero de un archivo, combinados como lo haría aplicar
/// cada uno con `UPSERT_SESSION`: fechas extremas, última rama, primer proyecto.
struct SessionAcc {
    started: i64,
    ended: i64,
    branch: Option<String>,
    project: Option<i64>,
    is_subagent: bool,
    merged: bool,
}

impl SessionAcc {
    fn new(ts: i64) -> Self {
        Self {
            started: ts,
            ended: ts,
            branch: None,
            project: None,
            is_subagent: false,
            merged: false,
        }
    }

    fn add(&mut self, s: &crate::providers::SessionRec, project: Option<i64>) {
        self.started = self.started.min(s.ts);
        self.ended = self.ended.max(s.ts);
        if s.git_branch.is_some() {
            self.branch.clone_from(&s.git_branch);
        }
        self.project = self.project.or(project);
        self.is_subagent |= s.is_subagent;
        self.merged = true;
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

    #[cfg(unix)]
    #[test]
    fn archivo_ilegible_entre_validos_cuenta_un_error_y_el_resto_se_importa() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, file, p, mut conn) = setup();
        let dir = file.parent().unwrap();
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            fs::write(
                dir.join(format!("{name}.jsonl")),
                line(&format!("m{i}"), 1, "2026-09-25T10:00:00Z") + "\n",
            )
            .unwrap();
        }
        let bad = dir.join("b.jsonl");
        fs::set_permissions(&bad, fs::Permissions::from_mode(0o000)).unwrap();
        if File::open(&bad).is_ok() {
            return; // root ignora los permisos
        }
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(p)];
        let mut stats = ScanStats::default();
        let out = crate::telemetry::tests::captured(None, || {
            stats = scan_all_with_threads(&mut conn, &providers, 4).unwrap();
        });
        assert_eq!((stats.files, stats.errors), (2, 1));
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM calls"), 2);
        assert!(out.contains(&bad.display().to_string()), "{out}");
    }

    /// Proveedor de prueba: `ok <id>` crea sesión y llamada; `huerfana <id>` crea una llamada
    /// de una sesión inexistente (viola la clave foránea al escribir).
    struct Fake(PathBuf);

    impl Provider for Fake {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn name(&self) -> &'static str {
            "Fake"
        }
        fn log_roots(&self) -> Vec<PathBuf> {
            vec![self.0.clone()]
        }
        fn matches(&self, path: &Path) -> bool {
            path.extension().is_some_and(|e| e == "log")
        }
        fn parse_line(&self, _path: &Path, line: &str) -> Result<Vec<Record>> {
            use crate::providers::{CallRec, SessionRec};
            let (kind, id) = line.split_once(' ').unwrap_or((line, ""));
            let call = |session: &str| {
                Record::Call(CallRec {
                    message_id: id.to_string(),
                    session_id: session.to_string(),
                    ts: 1,
                    model: "m".into(),
                    ..Default::default()
                })
            };
            Ok(match kind {
                "ok" => vec![
                    Record::Session(SessionRec {
                        id: format!("s-{id}"),
                        cwd: None,
                        git_branch: None,
                        ts: 1,
                        is_subagent: false,
                    }),
                    call(&format!("s-{id}")),
                ],
                _ => vec![call("fantasma")],
            })
        }
    }

    #[test]
    fn un_fallo_al_escribir_deshace_solo_ese_archivo() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.log"), "ok a1\n").unwrap();
        fs::write(dir.path().join("b.log"), "ok b1\nhuerfana b2\n").unwrap();
        fs::write(dir.path().join("c.log"), "ok c1\n").unwrap();
        let mut conn = db::open_in_memory().unwrap();
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(Fake(dir.path().to_path_buf()))];
        let stats = scan_all_with_threads(&mut conn, &providers, 2).unwrap();
        assert_eq!((stats.files, stats.errors), (2, 1));
        let ids: Vec<String> = conn
            .prepare("SELECT message_id FROM calls ORDER BY 1")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(ids, ["a1", "c1"]);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM file_state"), 2);
    }

    #[test]
    fn una_sesion_larga_se_escribe_con_un_insert_y_un_upsert_final() {
        let (_d, file, p, mut conn) = setup();
        let mut body = String::new();
        for i in 0..1000usize {
            let secs = (i * 7919) % 1000;
            let branch = match i % 3 {
                0 => String::new(),
                1 => r#""gitBranch":"","#.to_string(),
                _ => format!(r#""gitBranch":"b{i}","#),
            };
            let (model, side) = match i {
                998 => ("claude-opus-4-1", false),
                999 => ("claude-haiku-4-5", true),
                _ => ("claude-sonnet-4-5", false),
            };
            body.push_str(&format!(
                r#"{{"type":"assistant",{branch}"isSidechain":{side},"sessionId":"s1","timestamp":"2026-09-25T10:{:02}:{:02}Z","cwd":"/h/Proyectos/AgentBoard","message":{{"model":"{model}","id":"m{i}","content":[],"usage":{{"input_tokens":1,"output_tokens":1}}}}}}"#,
                secs / 60,
                secs % 60
            ));
            body.push('\n');
        }
        fs::write(&file, body).unwrap();
        let log = db::trace::start(&conn);
        ingest_file(&mut conn, &p, &file).unwrap();
        let log = db::trace::take(log);
        let runs = |prefix: &str| {
            log.iter()
                .filter(|e| e.sql.trim_start().starts_with(prefix))
                .count()
        };
        assert!(
            runs("INSERT INTO sessions") <= 2,
            "{}",
            runs("INSERT INTO sessions")
        );
        assert!(runs("UPDATE sessions SET model") <= 1);
        let row: (i64, i64, Option<String>, String, i64, String) = conn
            .query_row(
                "SELECT s.started_at, s.ended_at, s.git_branch, p.cwd, s.is_subagent, s.model
                 FROM sessions s JOIN projects p ON p.id = s.project_id",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .unwrap();
        let base = crate::providers::parse_ts("2026-09-25T10:00:00Z").unwrap();
        assert_eq!(
            row,
            (
                base,
                base + 999_000,
                Some("b998".to_string()),
                "/h/Proyectos/AgentBoard".to_string(),
                0,
                crate::pricing::normalize_model("claude-opus-4-1"),
            )
        );
    }

    #[test]
    fn la_sesion_acumulada_respeta_lo_que_ya_habia_en_la_base() {
        let (_d, file, p, mut conn) = setup();
        let with_branch = |id: &str, ts: &str, branch: &str| {
            line(id, 1, ts).replace(r#""cwd""#, &format!(r#""gitBranch":"{branch}","cwd""#))
        };
        fs::write(
            &file,
            with_branch("m1", "2026-09-25T10:00:00Z", "main") + "\n",
        )
        .unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
        writeln!(f, "{}", line("m2", 1, "2026-09-25T09:00:00Z")).unwrap();
        writeln!(f, "{}", line("m3", 1, "2026-09-25T11:00:00Z")).unwrap();
        ingest_file(&mut conn, &p, &file).unwrap();
        let row: (i64, i64, Option<String>) = conn
            .query_row(
                "SELECT started_at, ended_at, git_branch FROM sessions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        let ts = |s: &str| crate::providers::parse_ts(s).unwrap();
        assert_eq!(
            row,
            (
                ts("2026-09-25T09:00:00Z"),
                ts("2026-09-25T11:00:00Z"),
                Some("main".to_string())
            ),
            "una pasada sin rama no borra la que había"
        );
    }

    #[test]
    fn hilos_por_defecto_los_del_procesador() {
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        assert_eq!(default_threads(), cores);
    }

    #[test]
    fn la_ventana_retiene_a_los_lectores_adelantados() {
        use std::sync::mpsc::channel;
        use std::time::Duration;
        let window = std::sync::Arc::new(Window::new(2));
        assert!(
            window.wait_for(0) && window.wait_for(1),
            "dentro de la ventana"
        );
        let (tx, rx) = channel();
        let w = window.clone();
        let reader = std::thread::spawn(move || tx.send(w.wait_for(2)).unwrap());
        assert!(
            rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "el archivo 2 espera a que se recoja el 0"
        );
        window.advance(1);
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(true));
        reader.join().unwrap();

        let (tx, rx) = channel();
        let w = window.clone();
        let reader = std::thread::spawn(move || tx.send(w.wait_for(10)).unwrap());
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        window.close();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)),
            Ok(false),
            "cerrada: no se lee más"
        );
        reader.join().unwrap();
        assert!(!window.wait_for(0), "tras cerrar, nada");
    }

    #[test]
    fn el_escaneo_lee_lo_anadido_y_los_archivos_rotados() {
        let (_d, file, p, mut conn) = setup();
        let providers: Vec<Box<dyn Provider>> =
            vec![Box::new(ClaudeCode::with_roots(p.log_roots()))];
        let one = line("m1", 1, "2026-09-25T10:00:00Z") + "\n";
        fs::write(&file, &one).unwrap();
        assert_eq!(scan_all(&mut conn, &providers).unwrap().lines, 1);
        assert_eq!(
            scan_all(&mut conn, &providers).unwrap().lines,
            0,
            "sin cambios"
        );

        let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
        writeln!(f, "{}", line("m2", 1, "2026-09-25T10:01:00Z")).unwrap();
        drop(f);
        assert_eq!(
            scan_all(&mut conn, &providers).unwrap().lines,
            1,
            "solo lo añadido"
        );

        // Otro archivo con la misma ruta y el mismo tamaño: se relee entero.
        let rotated = file.with_extension("tmp");
        let same_size = [
            line("m3", 1, "2026-09-25T10:00:00Z"),
            line("m4", 1, "2026-09-25T10:01:00Z"),
        ]
        .join("\n")
            + "\n";
        assert_eq!(same_size.len() as u64, fs::metadata(&file).unwrap().len());
        fs::write(&rotated, same_size).unwrap();
        fs::rename(&rotated, &file).unwrap();
        assert_eq!(scan_all(&mut conn, &providers).unwrap().lines, 2);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM calls"), 4);
    }

    #[test]
    fn el_estado_guarda_tamano_y_fecha_y_el_resultado_las_lineas_malas() {
        let (_d, file, p, mut conn) = setup();
        let body = line("m1", 1, "2026-09-25T10:00:00Z") + "\n{roto\n";
        fs::write(&file, &body).unwrap();
        let r = ingest_file(&mut conn, &p, &file).unwrap();
        assert_eq!((r.lines, r.bad_lines, r.records), (2, 1, 2));
        let (size, offset, mtime): (i64, i64, i64) = conn
            .query_row("SELECT size, offset, mtime FROM file_state", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!((size, offset), (body.len() as i64, body.len() as i64));
        assert_eq!(mtime, mtime_ms(&fs::metadata(&file).unwrap()));
        assert!(mtime > 0);
    }

    #[test]
    fn la_sesion_acumulada_combina_como_los_upserts() {
        use crate::providers::SessionRec;
        let rec = |ts: i64, branch: Option<&str>, is_subagent: bool| SessionRec {
            id: "s".into(),
            cwd: None,
            git_branch: branch.map(str::to_string),
            ts,
            is_subagent,
        };
        let mut acc = SessionAcc::new(5);
        assert!(!acc.merged);
        acc.add(&rec(3, Some("a"), true), None);
        acc.add(&rec(9, None, false), Some(7));
        acc.add(&rec(4, Some("b"), false), Some(8));
        assert_eq!(
            (acc.started, acc.ended, acc.branch.as_deref(), acc.project),
            (3, 9, Some("b"), Some(7))
        );
        assert!(acc.is_subagent && acc.merged);
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
