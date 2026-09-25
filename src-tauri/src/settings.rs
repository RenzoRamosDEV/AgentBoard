//! Preferencias del usuario guardadas en la tabla `settings` (un JSON por clave).

use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Presupuesto mensual en USD; `None` = sin presupuesto.
    pub monthly_budget: Option<f64>,
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if let Some(b) = self.monthly_budget {
            if !b.is_finite() || b < 0.0 {
                bail!("el presupuesto debe ser un número mayor o igual que 0");
            }
        }
        Ok(())
    }
}

pub fn load(conn: &Connection) -> Result<Settings> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| r.get(0))
        .optional()?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
}

pub fn save(conn: &Connection, s: &Settings) -> Result<()> {
    s.validate()?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('app', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![serde_json::to_string(s)?],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn presupuesto_persiste() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        save(&db::open(&path).unwrap(), &Settings { monthly_budget: Some(50.0) }).unwrap();
        assert_eq!(load(&db::open(&path).unwrap()).unwrap().monthly_budget, Some(50.0));
    }

    #[test]
    fn negativo_se_rechaza_y_se_mantiene_el_anterior() {
        let conn = db::open_in_memory().unwrap();
        save(&conn, &Settings { monthly_budget: Some(50.0) }).unwrap();
        assert!(save(&conn, &Settings { monthly_budget: Some(-1.0) }).is_err());
        assert_eq!(load(&conn).unwrap().monthly_budget, Some(50.0));
    }
}
