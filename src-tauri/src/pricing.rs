//! Tabla de precios embebida (USD por millón de tokens) y normalización de nombres de modelo.
//!
//! Los precios de Claude salen de la tabla oficial de modelos (caché de 2026-06-24);
//! escritura de caché = 1,25× entrada (5 min) y 2× entrada (1 h). Los de OpenAI y Gemini
//! son aproximados. Una fase posterior los actualizará desde LiteLLM sin reimportar.

use anyhow::Result;
use rusqlite::{params, Connection};

/// (modelo, entrada, salida, lectura de caché, escritura 5 min, escritura 1 h)
pub const DEFAULT_PRICES: &[(&str, f64, f64, f64, f64, f64)] = &[
    ("claude-fable-5-1", 10.0, 50.0, 0.25, 12.5, 20.0),
    ("claude-fable-5", 10.0, 50.0, 1.0, 12.5, 20.0),
    ("claude-mythos-5-1", 10.0, 50.0, 1.0, 12.5, 20.0),
    ("claude-opus-5-5", 4.0, 20.0, 0.20, 5.0, 8.0),
    ("claude-opus-5", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-8", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-7", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-6", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-5", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-1", 15.0, 75.0, 1.5, 18.75, 30.0),
    ("claude-opus-4", 15.0, 75.0, 1.5, 18.75, 30.0),
    ("claude-sonnet-5", 2.0, 10.0, 0.2, 2.5, 4.0),
    ("claude-sonnet-4-6", 3.0, 15.0, 0.3, 3.75, 6.0),
    ("claude-sonnet-4-5", 3.0, 15.0, 0.3, 3.75, 6.0),
    ("claude-sonnet-4", 3.0, 15.0, 0.3, 3.75, 6.0),
    ("claude-haiku-4-5", 1.0, 5.0, 0.1, 1.25, 2.0),
    ("claude-3-5-haiku", 0.8, 4.0, 0.08, 1.0, 1.6),
    ("gpt-5", 1.25, 10.0, 0.125, 0.0, 0.0),
    ("gpt-5-codex", 1.25, 10.0, 0.125, 0.0, 0.0),
    ("gpt-5-mini", 0.25, 2.0, 0.025, 0.0, 0.0),
    ("gemini-2.5-pro", 1.25, 10.0, 0.31, 0.0, 0.0),
    ("gemini-2.5-flash", 0.30, 2.50, 0.075, 0.0, 0.0),
];

/// Carga los precios por defecto si la tabla está vacía.
pub fn seed_if_empty(conn: &Connection) -> Result<()> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM prices", [], |r| r.get(0))?;
    if n > 0 {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO prices (model, valid_from, input, output, cache_read, cache_write, cache_write_1h)
             VALUES (?1, 0, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for (model, i, o, cr, cw, cw1h) in DEFAULT_PRICES {
            stmt.execute(params![model, i, o, cr, cw, cw1h])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Nombre canónico para buscar precio: sin prefijo de proveedor, sin sufijo de fecha
/// (`-20251101`) ni marcas de contexto (`[1m]`).
pub fn normalize_model(raw: &str) -> String {
    let mut m = raw.trim().to_ascii_lowercase();
    if let Some(pos) = m.rfind('/') {
        m = m[pos + 1..].to_string();
    }
    if let Some(pos) = m.find('[') {
        m.truncate(pos);
    }
    if let Some((head, tail)) = m.rsplit_once('-') {
        if tail.len() == 8 && tail.chars().all(|c| c.is_ascii_digit()) {
            m = head.to_string();
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn insert_call(conn: &Connection, id: &str, model: &str, input: i64, output: i64) {
        conn.execute_batch(
            "INSERT OR IGNORE INTO agents VALUES ('a','A','/',0);
             INSERT OR IGNORE INTO sessions (id, agent_id, started_at, ended_at) VALUES ('s','a',0,0);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO calls (message_id, session_id, ts, model, input_tokens, output_tokens)
             VALUES (?1, 's', 1000, ?2, ?3, ?4)",
            params![id, model, input, output],
        )
        .unwrap();
    }

    fn cost(conn: &Connection, id: &str) -> f64 {
        conn.query_row("SELECT cost_usd FROM call_costs WHERE message_id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn coste_con_precio_conocido() {
        let conn = db::open_in_memory().unwrap();
        insert_call(&conn, "a", "claude-sonnet-4-5", 1_000_000, 100_000);
        assert!((cost(&conn, "a") - 4.5).abs() < 1e-9);
    }

    #[test]
    fn modelo_sin_precio_cuesta_cero() {
        let conn = db::open_in_memory().unwrap();
        insert_call(&conn, "b", "modelo-inventado", 1000, 1000);
        assert_eq!(cost(&conn, "b"), 0.0);
        let has: bool = conn
            .query_row("SELECT has_price FROM call_costs WHERE message_id='b'", [], |r| r.get(0))
            .unwrap();
        assert!(!has);
    }

    #[test]
    fn nuevo_precio_recalcula_sin_reimportar() {
        let conn = db::open_in_memory().unwrap();
        insert_call(&conn, "c", "claude-sonnet-4-5", 1_000_000, 0);
        assert!((cost(&conn, "c") - 3.0).abs() < 1e-9);
        conn.execute(
            "INSERT INTO prices (model, valid_from, input, output) VALUES ('claude-sonnet-4-5', 500, 1.0, 1.0)",
            [],
        )
        .unwrap();
        assert!((cost(&conn, "c") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn la_base_nueva_trae_precios_de_claude() {
        let conn = db::open_in_memory().unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM prices WHERE model LIKE 'claude-%'", [], |r| r.get(0))
            .unwrap();
        assert!(n >= 10);
    }

    #[test]
    fn normaliza_nombres() {
        assert_eq!(normalize_model("claude-opus-4-5-20251101"), "claude-opus-4-5");
        assert_eq!(normalize_model("anthropic/claude-sonnet-4-5"), "claude-sonnet-4-5");
        assert_eq!(normalize_model("claude-opus-5-5[1m]"), "claude-opus-5-5");
        assert_eq!(normalize_model("gpt-5"), "gpt-5");
    }
}
