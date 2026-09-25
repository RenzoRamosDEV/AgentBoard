//! Base SQLite en memoria (nada se guarda en disco) y migraciones versionadas.

use anyhow::{Context, Result};
use rusqlite::Connection;
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
];

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
    migrate(&conn)?;
    crate::pricing::seed_if_empty(&conn)?;
    Ok(conn)
}

/// Base en memoria: la que usa la app. Se rellena al arrancar leyendo los logs del ordenador.
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&conn)?;
    crate::pricing::seed_if_empty(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<()> {
    // journal_mode devuelve una fila, así que se lee con query_row.
    let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(Duration::from_secs(5))?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
