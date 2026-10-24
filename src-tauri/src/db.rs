//! Base SQLite: caché reconstruible en disco o solo en memoria, lectores aparte del escritor y
//! migraciones versionadas.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// Migraciones en orden; la versión aplicada se guarda en `PRAGMA user_version`.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/0001_inicial.sql")),
    (2, include_str!("../migrations/0002_ajustes_y_ahorro.sql")),
    (
        3,
        include_str!("../migrations/0003_turnos_y_subagentes.sql"),
    ),
    (4, include_str!("../migrations/0004_coste_reportado.sql")),
    (5, include_str!("../migrations/0005_coste_precalculado.sql")),
    (6, include_str!("../migrations/0006_primera_llamada.sql")),
    (7, include_str!("../migrations/0007_dia_local.sql")),
    (8, include_str!("../migrations/0008_agregados_diarios.sql")),
    (9, include_str!("../migrations/0009_indices.sql")),
    (10, include_str!("../migrations/0010_cache_fiel.sql")),
];

/// Versión de esquema que espera esta versión de la app (la de la última migración).
pub const LATEST: i64 = MIGRATIONS[MIGRATIONS.len() - 1].0;

/// Sentencias preparadas que la conexión conserva compiladas.
pub const STATEMENT_CACHE: usize = 64;

/// Carpeta de datos de la app (`~/.local/share/agentboard` en Linux).
pub fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("agentboard"))
}

/// Ruta de la caché en disco.
pub fn default_cache_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join("cache.sqlite"))
}

/// Versiones anteriores guardaban una base en la carpeta de datos; se elimina si sigue ahí.
pub fn remove_legacy_db() {
    if let Some(dir) = data_dir() {
        remove_legacy_db_in(&dir);
    }
}

/// Borra las bases heredadas de `dir` y sus auxiliares; la caché y la carpeta se quedan.
pub fn remove_legacy_db_in(dir: &Path) {
    for name in ["agentboard.db", "agentburn.db"] {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(dir.join(format!("{name}{suffix}")));
        }
    }
}

/// Dónde guarda la app los datos leídos de los logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheMode {
    /// Caché reconstruible en este archivo (en WAL).
    Disco(PathBuf),
    /// Solo en memoria: al abrir la app se vuelven a leer todos los logs.
    Memoria,
}

/// Modo de la caché al arrancar. Con «Solo memoria» se borra cualquier caché que haya quedado en
/// disco (p. ej. si al activarla no se pudo borrar porque un lector la tenía abierta).
pub fn startup_mode(memory_only: bool, path: Option<PathBuf>) -> CacheMode {
    match (memory_only, path) {
        (true, Some(path)) => {
            delete_cache_files(&path);
            CacheMode::Memoria
        }
        (false, Some(path)) => CacheMode::Disco(path),
        (_, None) => CacheMode::Memoria,
    }
}

/// Marca de la caché en `cache_meta`: una base sin ella no la creó esta app.
const CACHE_FORMAT: &str = "agentboard-cache";

/// La base abierta: la conexión del escritor y dónde vive, para abrir lectores sobre ella.
pub struct Storage {
    pub writer: Connection,
    pub target: Target,
    pub mode: CacheMode,
}

impl Storage {
    /// Abre la caché pedida. En disco, una caché que no se puede abrir, no es una base SQLite,
    /// está dañada o es de otra versión de esquema se borra y se crea vacía; si ni así se puede
    /// (permisos, disco lleno), la sesión sigue en memoria. Avisa por stderr en ambos casos. Solo
    /// falla si SQLite no puede abrir ni una base en memoria.
    pub fn open(mode: CacheMode) -> Result<Storage> {
        if let CacheMode::Disco(path) = &mode {
            match open_disk(path) {
                Ok(writer) => {
                    return Ok(Storage {
                        writer,
                        target: Target::File(path.clone()),
                        mode,
                    })
                }
                Err(e) => tracing::warn!(
                    "agentboard: no se pudo crear la caché en {}; los datos quedan en memoria: {e:#}",
                    path.display()
                ),
            }
        }
        Self::memory()
    }

    fn memory() -> Result<Storage> {
        let target = Target::shared_memory();
        Ok(Storage {
            writer: open_writer(&target)?,
            target,
            mode: CacheMode::Memoria,
        })
    }

    /// Lectores sobre esta base.
    pub fn readers(&self, size: usize) -> Result<ReadPool> {
        ReadPool::open(&self.target, size)
    }
}

/// Borra la caché y sus archivos auxiliares (si no existen, nada).
pub fn delete_cache_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let file = PathBuf::from(format!("{}{suffix}", path.display()));
        if let Err(e) = std::fs::remove_file(&file) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("agentboard: no se pudo borrar {}: {e}", file.display());
            }
        }
    }
}

/// ¿Es una caché válida de esta versión? Lee lo justo para fallar si no es SQLite, si la versión
/// de esquema o la marca no cuadran o si las primeras páginas están dañadas.
fn validate(path: &Path) -> Result<()> {
    let conn = Connection::open(path)?;
    let version = schema_version(&conn)?;
    anyhow::ensure!(
        version == LATEST,
        "versión de esquema {version}, se espera {LATEST}"
    );
    let format: String = conn.query_row(
        "SELECT value FROM cache_meta WHERE key = 'format'",
        [],
        |r| r.get(0),
    )?;
    anyhow::ensure!(format == CACHE_FORMAT, "marca desconocida: {format}");
    let _: i64 = conn.query_row("SELECT COUNT(*) FROM file_state", [], |r| r.get(0))?;
    Ok(())
}

fn open_disk(path: &Path) -> Result<Connection> {
    let dir = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("ruta sin carpeta: {}", path.display()))?;
    create_private_dir(dir)?;
    if path.exists() {
        if let Err(e) = validate(path) {
            tracing::warn!(
                "agentboard: la caché {} no vale ({e:#}); se borra y se reconstruye desde los logs",
                path.display()
            );
            delete_cache_files(path);
        }
    }
    let fresh = !path.exists();
    if fresh {
        create_private_file(path)?;
    }
    let attempt = || -> Result<Connection> {
        let conn = open_writer(&Target::File(path.to_path_buf()))?;
        conn.execute(
            "INSERT OR IGNORE INTO cache_meta (key, value) VALUES ('format', ?1)",
            [CACHE_FORMAT],
        )?;
        Ok(conn)
    };
    match attempt() {
        Ok(conn) => Ok(conn),
        // Dañada más allá de lo que mira `validate`: se borra y se crea de nuevo una vez.
        Err(e) if !fresh => {
            tracing::warn!("agentboard: la caché no se pudo abrir ({e:#}); se reconstruye");
            delete_cache_files(path);
            create_private_file(path)?;
            attempt()
        }
        Err(e) => Err(e),
    }
}

/// Carpeta de datos solo para el usuario (`0700` en Unix).
fn create_private_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("no se pudo crear {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Archivo vacío solo para el usuario (`0600` en Unix): SQLite crea `-wal` y `-shm` con los
/// permisos del archivo principal.
fn create_private_file(path: &Path) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .with_context(|| format!("no se pudo crear {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Dónde vive la base: un archivo (en WAL) o una base en memoria con caché compartida, a la que
/// se conectan por nombre el escritor y los lectores; existe mientras viva la del escritor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    File(PathBuf),
    SharedMemory(String),
}

impl Target {
    /// Base en memoria compartida con un nombre que no repite ninguna otra de este proceso.
    pub fn shared_memory() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Self::SharedMemory(format!(
            "file:agentboard-{}-{n}?mode=memory&cache=shared",
            std::process::id()
        ))
    }

    fn connect(&self) -> Result<Connection> {
        match self {
            Self::File(path) => Connection::open(path)
                .with_context(|| format!("no se pudo abrir {}", path.display())),
            Self::SharedMemory(uri) => {
                Connection::open(uri).with_context(|| format!("no se pudo abrir {uri}"))
            }
        }
    }
}

/// Abre la conexión de escritura y deja la base lista (migraciones, precios, zona, derivados).
pub fn open_writer(target: &Target) -> Result<Connection> {
    let conn = target.connect()?;
    configure_writer(&conn, matches!(target, Target::File(_)))?;
    prepare(&conn)?;
    Ok(conn)
}

/// Abre una base en un archivo y aplica las migraciones pendientes.
pub fn open(path: &Path) -> Result<Connection> {
    open_writer(&Target::File(path.to_path_buf()))
}

/// Conjunto de lectores vigente, compartido por la interfaz, la bandeja, los avisos y el escritor
/// (que lo sustituye al reconstruir la caché).
pub type SharedReaders = std::sync::Arc<std::sync::RwLock<std::sync::Arc<ReadPool>>>;

/// ¿El error viene de una base dañada o que no es SQLite? Entonces hay que reconstruir la caché.
pub fn is_corruption(e: &anyhow::Error) -> bool {
    e.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(f, _))
                if matches!(f.code, rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase)
        )
    })
}

/// Conexiones de solo lectura para la interfaz, la bandeja y los avisos: no esperan al escritor
/// (archivo en WAL: ven el último lote confirmado) o esperan como mucho a un lote (memoria
/// compartida: `unlock_notify`), y nunca ven un lote a medias.
pub struct ReadPool {
    conns: Vec<Mutex<Connection>>,
}

impl ReadPool {
    /// Lectores por defecto: los comandos ya van a hebras aparte y casi todos responden en ms.
    pub const SIZE: usize = 2;

    /// Abre `size` lectores sobre una base que el escritor ya dejó preparada.
    pub fn open(target: &Target, size: usize) -> Result<Self> {
        let conns = (0..size.max(1))
            .map(|_| {
                let conn = target.connect()?;
                configure_reader(&conn)?;
                Ok(Mutex::new(conn))
            })
            .collect::<Result<_>>()?;
        Ok(Self { conns })
    }

    /// Ejecuta `f` con el primer lector libre o, si están todos ocupados, esperando al primero.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        for conn in &self.conns {
            if let Ok(guard) = conn.try_lock() {
                return f(&guard);
            }
        }
        let first = self
            .conns
            .first()
            .ok_or_else(|| anyhow::anyhow!("sin lectores"))?;
        let guard = first
            .lock()
            .map_err(|e| anyhow::anyhow!("lector envenenado: {e}"))?;
        f(&guard)
    }
}

/// Migraciones, precios por defecto, zona de la caché (la del sistema) y derivados pendientes.
fn prepare(conn: &Connection) -> Result<()> {
    migrate(conn)?;
    crate::pricing::seed_if_empty(conn)?;
    set_cache_tz(conn, &crate::tz::system_name())?;
    crate::derived::refresh(conn)?;
    Ok(())
}

/// Base privada en memoria, sin lectores aparte (servidor MCP, tests, `perf_scan`).
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure_writer(&conn, false)?;
    prepare(&conn)?;
    Ok(conn)
}

/// PRAGMAs del escritor (D3). En archivo, WAL con `synchronous=NORMAL`: un corte pierde como mucho
/// los últimos lotes, y `file_state` va en la misma transacción que sus filas, así que se releen.
fn configure_writer(conn: &Connection, file: bool) -> Result<()> {
    if file {
        // journal_mode devuelve una fila, así que se lee con query_row.
        let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "cache_size", -65_536)?;
    } else {
        conn.pragma_update(None, "synchronous", "OFF")?;
    }
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    // Claves foráneas activas: desactivarlas no mejoró el escaneo frío (paso 7, medido A/B) una vez
    // que el escritor inserta sin disparadores. El escritor además rechaza él los registros de una
    // sesión inexistente (`ingest::Writer`), así que el resultado no depende de este PRAGMA.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(Duration::from_secs(5))?;
    // La ingesta reutiliza ~16 sentencias y las consultas otras tantas (rusqlite trae 16).
    conn.set_prepared_statement_cache_capacity(STATEMENT_CACHE);
    Ok(())
}

fn configure_reader(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "query_only", 1)?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", -16_384)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.set_prepared_statement_cache_capacity(STATEMENT_CACHE);
    Ok(())
}

/// Zona IANA con la que se guardan los días locales (`calls.day`, `tool_calls.day`).
pub fn cache_tz(conn: &Connection) -> Result<String> {
    Ok(conn
        .prepare_cached("SELECT value FROM cache_meta WHERE key = 'tz'")?
        .query_row([], |r| r.get(0))
        .optional()?
        .unwrap_or_default())
}

pub fn cache_zone(conn: &Connection) -> Result<chrono_tz::Tz> {
    Ok(crate::tz::parse(&cache_tz(conn)?))
}

/// Fija la zona de la caché. Si cambia, todos los días quedan pendientes y se recalculan, y con
/// ellos todos los agregados diarios.
pub fn set_cache_tz(conn: &Connection, name: &str) -> Result<()> {
    if cache_tz(conn)? == name {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO cache_meta (key, value) VALUES ('tz', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [name],
    )?;
    tx.execute_batch(
        "DELETE FROM daily_stats; DELETE FROM tool_stats;
         UPDATE calls SET day = NULL; UPDATE tool_calls SET day = NULL;",
    )?;
    crate::derived::refresh(&tx)?;
    tx.commit()?;
    Ok(())
}

pub fn schema_version(conn: &Connection) -> Result<i64> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    let current = schema_version(conn)?;
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v > current) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)
            .with_context(|| format!("falló la migración {version}"))?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

/// Registro de las sentencias que ejecuta una conexión (solo tests): SQL y si el objeto de
/// sentencia se ejecutaba por primera vez, es decir, si acababa de compilarse.
#[cfg(test)]
pub mod trace {
    use rusqlite::{ffi, Connection};
    use std::ffi::{c_char, c_int, c_uint, c_void, CStr};
    use std::sync::Mutex;

    #[derive(Debug, Clone, PartialEq)]
    pub struct Executed {
        pub sql: String,
        pub compiled: bool,
    }

    pub type Log = Mutex<Vec<Executed>>;

    unsafe extern "C" fn on_statement(
        _mask: c_uint,
        ctx: *mut c_void,
        stmt: *mut c_void,
        text: *mut c_void,
    ) -> c_int {
        // Al entrar en un disparador SQLite avisa con la sentencia de fuera y el texto
        // «-- TRIGGER nombre»: no es otra ejecución de esa sentencia.
        if !text.is_null()
            && CStr::from_ptr(text as *const c_char)
                .to_bytes()
                .starts_with(b"--")
        {
            return 0;
        }
        let log = &*(ctx as *const Log);
        let stmt = stmt as *mut ffi::sqlite3_stmt;
        let sql = CStr::from_ptr(ffi::sqlite3_sql(stmt))
            .to_string_lossy()
            .into_owned();
        let runs = ffi::sqlite3_stmt_status(stmt, ffi::SQLITE_STMTSTATUS_RUN, 0);
        if let Ok(mut v) = log.lock() {
            v.push(Executed {
                sql,
                compiled: runs == 0,
            });
        }
        0
    }

    /// Empieza a registrar; el registro vive lo que el proceso de test (se filtra a propósito
    /// para que la conexión nunca apunte a memoria liberada).
    pub fn start(conn: &Connection) -> &'static Log {
        let log: &'static Log = Box::leak(Box::default());
        unsafe {
            ffi::sqlite3_trace_v2(
                conn.handle(),
                ffi::SQLITE_TRACE_STMT as c_uint,
                Some(on_statement),
                log as *const Log as *mut c_void,
            );
        }
        log
    }

    pub fn take(log: &Log) -> Vec<Executed> {
        std::mem::take(&mut *log.lock().unwrap())
    }

    const KEYWORDS: &[&str] = &[
        "where", "join", "left", "cross", "on", "group", "order", "limit", "set", "as", "inner",
    ];

    /// Nombres con los que `sql` usa `calls` o `tool_calls` (la tabla, la vista `call_costs` o
    /// un alias).
    fn big_table_names(sql: &str) -> Vec<String> {
        let words: Vec<String> = sql
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect();
        let mut names = vec![
            "calls".to_string(),
            "tool_calls".to_string(),
            "call_costs".to_string(),
        ];
        for (i, w) in words.iter().enumerate() {
            if names[..3].contains(w) {
                let mut j = i + 1;
                if words.get(j).map(String::as_str) == Some("as") {
                    j += 1;
                }
                if let Some(alias) = words.get(j).filter(|a| !KEYWORDS.contains(&a.as_str())) {
                    names.push(alias.clone());
                }
            }
        }
        names
    }

    /// Líneas del plan de `sql` que recorren `calls` o `tool_calls` enteras (con o sin índice),
    /// salvo los índices parciales de filas pendientes, que solo tienen lo de un lote.
    pub fn full_scans(conn: &Connection, sql: &str) -> Vec<String> {
        let names = big_table_names(sql);
        let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        let nulls = vec![rusqlite::types::Value::Null; stmt.parameter_count()];
        let detail: Vec<String> = stmt
            .query_map(rusqlite::params_from_iter(nulls), |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        detail
            .into_iter()
            .filter(|d| {
                let mut w = d.split_whitespace();
                w.next() == Some("SCAN")
                    && w.next().is_some_and(|t| names.contains(&t.to_lowercase()))
                    && !d.contains("_pending")
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_traza_distingue_compilar_de_reutilizar() {
        let conn = open_in_memory().unwrap();
        let log = trace::start(&conn);
        for _ in 0..3 {
            conn.execute("UPDATE settings SET value = 'x' WHERE key = 'k'", [])
                .unwrap();
        }
        assert_eq!(
            trace::take(log).iter().filter(|e| e.compiled).count(),
            3,
            "execute compila cada vez"
        );
        for _ in 0..3 {
            conn.prepare_cached("UPDATE settings SET value = 'y' WHERE key = 'k'")
                .unwrap()
                .execute([])
                .unwrap();
        }
        let runs = trace::take(log);
        assert_eq!(runs.len(), 3);
        assert_eq!(runs.iter().filter(|e| e.compiled).count(), 1);
        assert!(runs[0].sql.starts_with("UPDATE settings"));
    }

    #[test]
    fn migrar_dos_veces_no_falla() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let conn = open(&path).unwrap();
        conn.execute(
            "INSERT INTO agents (id, name, log_root, first_seen) VALUES ('x', 'X', '/', 0)",
            [],
        )
        .unwrap();
        drop(conn);
        let conn = open(&path).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), MIGRATIONS.last().unwrap().0);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM agents", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "los datos existentes se conservan");
    }

    #[test]
    fn base_de_fase_0_migra_sin_perder_datos() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATIONS[0].1).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute_batch(
            "INSERT INTO agents VALUES ('a','A','/',0);
             INSERT INTO sessions (id, agent_id, started_at, ended_at) VALUES ('s','a',0,0);
             INSERT INTO calls (message_id, session_id, ts, model, cache_read) VALUES ('m','s',1,'x',10);",
        )
        .unwrap();
        migrate(&conn).unwrap();
        let (n, savings): (i64, f64) = conn
            .query_row(
                "SELECT COUNT(*), SUM(cache_savings_usd) FROM call_costs",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((n, savings), (1, 0.0));
        assert_eq!(schema_version(&conn).unwrap(), MIGRATIONS.last().unwrap().0);
    }

    #[test]
    fn la_vista_de_costes_existe() {
        let conn = open_in_memory().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='view' AND name='call_costs'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    fn calls(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM calls", [], |r| r.get(0))
            .unwrap()
    }

    fn insert_batch(conn: &Connection, from: usize, n: usize) {
        conn.execute_batch(
            "INSERT OR IGNORE INTO agents VALUES ('a','A','/',0);
             INSERT OR IGNORE INTO sessions (id, agent_id, started_at, ended_at) VALUES ('s','a',0,0);",
        )
        .unwrap();
        for i in from..from + n {
            conn.execute(
                "INSERT INTO calls (message_id, session_id, ts, model) VALUES (?1, 's', 1, 'm')",
                [format!("m{i}")],
            )
            .unwrap();
        }
    }

    #[test]
    fn en_archivo_una_lectura_larga_no_frena_al_escritor_y_ve_su_foto() {
        let dir = tempfile::tempdir().unwrap();
        let target = Target::File(dir.path().join("cache.sqlite"));
        let writer = open_writer(&target).unwrap();
        insert_batch(&writer, 0, 10);
        let pool = std::sync::Arc::new(ReadPool::open(&target, 2).unwrap());
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let reader = {
            let pool = pool.clone();
            std::thread::spawn(move || {
                pool.with(|c| {
                    c.execute_batch("BEGIN")?;
                    let before = calls(c);
                    started_tx.send(()).unwrap();
                    go_rx.recv().unwrap();
                    let during = calls(c);
                    c.execute_batch("COMMIT")?;
                    Ok((before, during, calls(c)))
                })
                .unwrap()
            })
        };
        started_rx.recv().unwrap();
        // Lote del escritor con la lectura abierta: confirma sin esperar.
        let t = std::time::Instant::now();
        let tx = writer.unchecked_transaction().unwrap();
        insert_batch(&tx, 10, 5);
        tx.commit().unwrap();
        assert!(
            t.elapsed() < Duration::from_secs(1),
            "el escritor esperó a la lectura"
        );
        go_tx.send(()).unwrap();
        assert_eq!(reader.join().unwrap(), (10, 10, 15));
    }

    #[test]
    fn en_memoria_compartida_el_lector_espera_al_lote_y_lo_ve_entero() {
        let target = Target::shared_memory();
        let writer = open_writer(&target).unwrap();
        insert_batch(&writer, 0, 10);
        let pool = std::sync::Arc::new(ReadPool::open(&target, 1).unwrap());
        assert_eq!(pool.with(|c| Ok(calls(c))).unwrap(), 10);
        let tx = writer.unchecked_transaction().unwrap();
        insert_batch(&tx, 10, 5);
        let reader = {
            let pool = pool.clone();
            std::thread::spawn(move || {
                let t = std::time::Instant::now();
                let n = pool.with(|c| Ok(calls(c)));
                (n.map_err(|e| e.to_string()), t.elapsed())
            })
        };
        std::thread::sleep(Duration::from_millis(300));
        tx.commit().unwrap();
        let (n, waited) = reader.join().unwrap();
        assert_eq!(n, Ok(15), "sin SQLITE_LOCKED y con el lote entero");
        assert!(waited >= Duration::from_millis(250), "{waited:?}");
    }

    #[test]
    fn los_lectores_no_pueden_escribir() {
        for target in [
            Target::shared_memory(),
            Target::File(tempfile::tempdir().unwrap().keep().join("c.sqlite")),
        ] {
            let _writer = open_writer(&target).unwrap();
            let pool = ReadPool::open(&target, 2).unwrap();
            let err = pool
                .with(|c| Ok(c.execute("INSERT INTO agents VALUES ('x','X','/',0)", [])?))
                .unwrap_err();
            assert!(format!("{err:#}").contains("readonly"), "{err:#}");
        }
    }

    // ------------------------------------------------------------ caché en disco (D2)

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn cache_in(dir: &Path) -> PathBuf {
        dir.join("datos").join("agentboard").join("cache.sqlite")
    }

    #[test]
    fn la_cache_en_disco_se_crea_en_wal_y_solo_para_el_usuario() {
        let dir = tempfile::tempdir().unwrap();
        let path = cache_in(dir.path());
        let storage = Storage::open(CacheMode::Disco(path.clone())).unwrap();
        assert_eq!(storage.mode, CacheMode::Disco(path.clone()));
        let journal: String = storage
            .writer
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(journal, "wal");
        insert_batch(&storage.writer, 0, 3);
        let _readers = storage.readers(1).unwrap();
        #[cfg(unix)]
        {
            assert_eq!(mode(&path), 0o600);
            for aux in ["-wal", "-shm"] {
                let aux = PathBuf::from(format!("{}{aux}", path.display()));
                assert!(aux.exists(), "{}", aux.display());
                assert_eq!(mode(&aux), 0o600, "{}", aux.display());
            }
            assert_eq!(mode(path.parent().unwrap()), 0o700);
        }
    }

    #[test]
    fn reabrir_la_cache_conserva_los_datos_y_el_estado_de_lectura() {
        use crate::providers::{claude_code::ClaudeCode, Provider};
        let dir = tempfile::tempdir().unwrap();
        let path = cache_in(dir.path());
        let fx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/claude_code");
        let providers: Vec<Box<dyn Provider>> =
            vec![Box::new(ClaudeCode::with_roots(vec![fx.clone()]))];
        let first = {
            let mut storage = Storage::open(CacheMode::Disco(path.clone())).unwrap();
            let stats = crate::ingest::scan_all(&mut storage.writer, &providers).unwrap();
            assert!(stats.records > 0);
            (calls(&storage.writer), stats.files)
        };
        let mut storage = Storage::open(CacheMode::Disco(path.clone())).unwrap();
        assert_eq!(calls(&storage.writer), first.0, "los datos siguen ahí");
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(ClaudeCode::with_roots(vec![fx]))];
        let again = crate::ingest::scan_all(&mut storage.writer, &providers).unwrap();
        assert_eq!(
            (again.files, again.lines, again.records),
            (first.1, 0, 0),
            "no lee nada"
        );
    }

    #[test]
    fn una_cache_danada_o_de_otra_version_se_borra_y_se_recrea_vacia() {
        let dir = tempfile::tempdir().unwrap();
        let path = cache_in(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let wal = PathBuf::from(format!("{}-wal", path.display()));
        type Damage<'a> = (&'a str, &'a dyn Fn(&Path));
        let damage: [Damage; 3] = [
            ("bytes basura", &|p: &Path| {
                std::fs::write(
                    p,
                    b"esto no es una base SQLite, son bytes de relleno ".repeat(200),
                )
                .unwrap()
            }),
            ("base truncada", &|p: &Path| {
                let len = std::fs::metadata(p).unwrap().len();
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(p)
                    .unwrap()
                    .set_len(len / 3)
                    .unwrap();
            }),
            ("otra versión", &|p: &Path| {
                Connection::open(p)
                    .unwrap()
                    .pragma_update(None, "user_version", 99)
                    .unwrap();
            }),
        ];
        for (what, damage) in damage {
            {
                let storage = Storage::open(CacheMode::Disco(path.clone())).unwrap();
                insert_batch(&storage.writer, 0, 2000);
                storage
                    .writer
                    .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
                    .unwrap();
            }
            let _ = std::fs::remove_file(&wal);
            damage(&path);
            let storage = Storage::open(CacheMode::Disco(path.clone())).unwrap();
            assert_eq!(storage.mode, CacheMode::Disco(path.clone()), "{what}");
            assert_eq!(calls(&storage.writer), 0, "{what}: recreada vacía");
            assert_eq!(schema_version(&storage.writer).unwrap(), LATEST, "{what}");
        }
    }

    #[test]
    fn una_cache_que_valida_pero_no_abre_se_recrea_en_disco() {
        let dir = tempfile::tempdir().unwrap();
        let path = cache_in(dir.path());
        {
            let storage = Storage::open(CacheMode::Disco(path.clone())).unwrap();
            insert_batch(&storage.writer, 0, 3);
            // Le falta una tabla que `validate` no mira pero el escritor usa al abrir.
            storage
                .writer
                .execute_batch("DROP TABLE daily_stats")
                .unwrap();
        }
        let storage = Storage::open(CacheMode::Disco(path.clone())).unwrap();
        assert_eq!(storage.mode, CacheMode::Disco(path), "sigue en disco");
        assert_eq!(calls(&storage.writer), 0, "recreada vacía");
    }

    #[cfg(unix)]
    #[test]
    fn sin_permiso_de_escritura_la_cache_queda_en_memoria() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("bloqueada");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
        let path = locked.join("agentboard").join("cache.sqlite");
        let storage = Storage::open(CacheMode::Disco(path.clone())).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(storage.mode, CacheMode::Memoria);
        insert_batch(&storage.writer, 0, 1);
        assert_eq!(
            storage.readers(1).unwrap().with(|c| Ok(calls(c))).unwrap(),
            1
        );
        assert!(!path.exists());
    }

    #[test]
    fn la_limpieza_de_bases_heredadas_conserva_la_cache_y_la_carpeta() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("agentboard");
        std::fs::create_dir_all(&data).unwrap();
        for name in ["agentboard.db", "agentburn.db"] {
            for suffix in ["", "-wal", "-shm"] {
                std::fs::write(data.join(format!("{name}{suffix}")), b"x").unwrap();
            }
        }
        std::fs::write(data.join("cache.sqlite"), b"x").unwrap();
        remove_legacy_db_in(&data);
        let left: Vec<String> = std::fs::read_dir(&data)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec!["cache.sqlite".to_string()]);
    }

    #[test]
    fn con_solo_memoria_al_arrancar_se_borra_la_cache_que_quede() {
        let dir = tempfile::tempdir().unwrap();
        let path = cache_in(dir.path());
        drop(Storage::open(CacheMode::Disco(path.clone())).unwrap());
        std::fs::write(format!("{}-wal", path.display()), b"x").unwrap();
        assert_eq!(
            startup_mode(false, Some(path.clone())),
            CacheMode::Disco(path.clone())
        );
        assert!(path.exists());
        assert_eq!(startup_mode(true, Some(path.clone())), CacheMode::Memoria);
        assert!(!path.exists());
        assert!(!PathBuf::from(format!("{}-wal", path.display())).exists());
        assert_eq!(startup_mode(false, None), CacheMode::Memoria);
    }

    #[test]
    fn modo_wal_en_archivo() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("t.db")).unwrap();
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }
}
