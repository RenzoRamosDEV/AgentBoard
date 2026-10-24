//! Datos derivados que se recalculan al final de cada transacción de escritura, solo para las
//! filas que quedaron pendientes: el coste de las llamadas nuevas escritas sin él o con valores o
//! precios cambiados (`has_price` nulo, ver la migración 0005).

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

/// Recalcula lo pendiente. Va dentro de la transacción del llamador, antes de confirmarla.
pub fn refresh(conn: &Connection) -> Result<()> {
    conn.prepare_cached(REFRESH_COSTS)?.execute([])?;
    Ok(())
}
