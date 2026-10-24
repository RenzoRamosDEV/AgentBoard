//! Datos derivados que se recalculan al final de cada transacción de escritura, solo para lo que
//! quedó pendiente: el coste de las llamadas escritas sin él o con valores o precios cambiados
//! (`has_price` nulo, migración 0005), el día local de las filas que no lo tienen (`day` nulo,
//! migración 0007) y la primera llamada de las sesiones anotadas en `dirty_sessions` (0006).

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use anyhow::Result;
use rusqlite::Connection;

/// Coste con el precio vigente en la fecha de cada llamada: la fórmula de la vista `call_costs`
/// de la migración 0004, con el mismo orden de operaciones (mismo resultado en coma flotante).
/// `ingest::INSERT_CALL` la repite con parámetros en lugar de columnas.
const REFRESH_COSTS: &str = "UPDATE calls SET (has_price, cost_usd, cache_savings_usd) = (
       SELECT p.model IS NOT NULL OR calls.cost_reported IS NOT NULL,
         CASE WHEN p.model IS NOT NULL THEN (
           calls.input_tokens * p.input +
           calls.output_tokens * p.output +
           calls.cache_read * p.cache_read +
           (calls.cache_write - calls.cache_write_1h) * p.cache_write +
           calls.cache_write_1h * p.cache_write_1h
         ) / 1e6 ELSE COALESCE(calls.cost_reported, 0) END,
         COALESCE(calls.cache_read * (p.input - p.cache_read) / 1e6, 0)
       FROM (SELECT 1) LEFT JOIN prices p ON p.model = calls.model
        AND p.valid_from = (SELECT MAX(valid_from) FROM prices
                            WHERE model = calls.model AND valid_from <= calls.ts))
     WHERE has_price IS NULL";

/// Las filas pendientes cambiaron o son nuevas: su sesión también.
const MARK_PENDING_SESSIONS: &str = "INSERT OR IGNORE INTO dirty_sessions
     SELECT DISTINCT session_id FROM calls WHERE has_price IS NULL";

/// Primera llamada principal: menor `ts` y, con el mismo `ts`, la que se escribió antes.
const REFRESH_FIRST_CALLS: &str = "UPDATE sessions SET first_call_id = (
       SELECT message_id FROM calls c WHERE c.session_id = sessions.id AND c.is_sidechain = 0
       ORDER BY c.ts, c.rowid LIMIT 1)
     WHERE id IN (SELECT session_id FROM dirty_sessions)";

/// Calcula el día local (zona de la caché) de las filas de `table` que no lo tienen.
fn fill_days(conn: &Connection, table: &str, zone: &chrono_tz::Tz) -> Result<()> {
    let pending: Vec<(i64, i64)> = conn
        .prepare_cached(&format!("SELECT rowid, ts FROM {table} WHERE day IS NULL"))?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut update =
        conn.prepare_cached(&format!("UPDATE {table} SET day = ?1 WHERE rowid = ?2"))?;
    for (rowid, ts) in pending {
        update.execute([crate::tz::day_number(ts, zone), rowid])?;
    }
    Ok(())
}

/// Recalcula lo pendiente. Va dentro de la transacción del llamador, antes de confirmarla.
pub fn refresh(conn: &Connection) -> Result<()> {
    conn.prepare_cached(MARK_PENDING_SESSIONS)?.execute([])?;
    conn.prepare_cached(REFRESH_COSTS)?.execute([])?;
    let zone = crate::db::cache_zone(conn)?;
    fill_days(conn, "calls", &zone)?;
    fill_days(conn, "tool_calls", &zone)?;
    conn.prepare_cached(REFRESH_FIRST_CALLS)?.execute([])?;
    conn.prepare_cached("DELETE FROM dirty_sessions")?
        .execute([])?;
    Ok(())
}

/// Anota sesiones con llamadas nuevas escritas con su coste (el escritor de la ingesta).
pub fn mark_sessions<'a>(
    conn: &Connection,
    sessions: impl IntoIterator<Item = &'a str>,
) -> Result<()> {
    let mut stmt = conn.prepare_cached("INSERT OR IGNORE INTO dirty_sessions VALUES (?1)")?;
    for s in sessions {
        stmt.execute([s])?;
    }
    Ok(())
}
