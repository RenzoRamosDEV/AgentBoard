//! Base SQLite en memoria (nada se guarda en disco) y migraciones versionadas.

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
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
];

/// Sentencias preparadas que la conexión conserva compiladas.
pub const STATEMENT_CACHE: usize = 64;

/// Versiones anteriores guardaban una base en la carpeta de datos; se elimina si sigue ahí.
pub fn remove_legacy_db() {
    let Some(dir) = dirs::data_dir().map(|d| d.join("agentboard")) else {
        return;
    };
    for name in ["agentboard.db", "agentburn.db"] {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(dir.join(format!("{name}{suffix}")));
        }
    }
    let _ = std::fs::remove_dir(&dir);
}

/// Abre una base en un archivo (solo tests) y aplica las migraciones pendientes.
pub fn open(path: &Path) -> Result<Connection> {
    let conn =
        Connection::open(path).with_context(|| format!("no se pudo abrir {}", path.display()))?;
    configure(&conn)?;
    prepare(&conn)?;
    Ok(conn)
}

/// Migraciones, precios por defecto, zona de la caché (la del sistema) y derivados pendientes.
fn prepare(conn: &Connection) -> Result<()> {
    migrate(conn)?;
    crate::pricing::seed_if_empty(conn)?;
    set_cache_tz(conn, &crate::tz::system_name())?;
    crate::derived::refresh(conn)?;
    Ok(())
}

/// Base en memoria: la que usa la app. Se rellena al arrancar leyendo los logs del ordenador.
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    prepare(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<()> {
    // journal_mode devuelve una fila, así que se lee con query_row.
    let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(Duration::from_secs(5))?;
    // La ingesta reutiliza ~16 sentencias y las consultas otras tantas (rusqlite trae 16).
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

/// Fija la zona de la caché. Si cambia, todos los días quedan pendientes y se recalculan.
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
    tx.execute_batch("UPDATE calls SET day = NULL; UPDATE tool_calls SET day = NULL;")?;
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
