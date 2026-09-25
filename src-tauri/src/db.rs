//! Conexión SQLite, ubicación de la base y migraciones versionadas.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Migraciones en orden; la versión aplicada se guarda en `PRAGMA user_version`.
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../migrations/0001_inicial.sql"))];

/// Carpeta de datos de AgentBurn en este sistema (`~/.local/share/agentburn` en Linux).
pub fn data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().context("no se encontró la carpeta de datos del sistema")?;
    Ok(base.join("agentburn"))
}

/// Ruta de la base por defecto, creando la carpeta si falta.
pub fn default_db_path() -> Result<PathBuf> {
    let dir = data_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("no se pudo crear {}", dir.display()))?;
    Ok(dir.join("agentburn.db"))
}

/// Abre (o crea) la base, activa WAL y aplica las migraciones pendientes.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path).with_context(|| format!("no se pudo abrir {}", path.display()))?;
    configure(&conn)?;
    migrate(&conn)?;
    crate::pricing::seed_if_empty(&conn)?;
    Ok(conn)
}

/// Base en memoria para tests.
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
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM agents", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "los datos existentes se conservan");
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
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, "wal");
    }
}
