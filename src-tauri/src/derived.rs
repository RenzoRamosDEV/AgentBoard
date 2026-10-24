//! Datos derivados que se recalculan al final de cada transacción de escritura, solo para lo que
//! quedó pendiente: el coste de las llamadas escritas sin él o con valores o precios cambiados
//! (`has_price` nulo, migración 0005), el día local de las filas que no lo tienen (`day` nulo,
//! migración 0007), la primera llamada de cada sesión (0006) y los agregados diarios de las
//! claves `(sesión, día)` anotadas en `dirty_keys` (0008).

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};

/// Coste con el precio vigente en la fecha de cada llamada: la fórmula de la vista `call_costs`
/// de la migración 0004, con el mismo orden de operaciones (mismo resultado en coma flotante).
/// `pricing::PriceBook` la repite en Rust para el escritor de la ingesta.
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

/// Las filas con coste pendiente cambiaron o son nuevas: su clave también.
const MARK_PENDING_COSTS: &str = "INSERT OR IGNORE INTO dirty_keys
     SELECT session_id, IFNULL(day, 0) FROM calls WHERE has_price IS NULL";

/// Primera llamada principal: menor `ts` y, con el mismo `ts`, la que se escribió antes.
const FIRST_CALL: &str = "SELECT message_id, day FROM calls
     WHERE session_id = ?1 AND is_sidechain = 0 ORDER BY ts, rowid LIMIT 1";

// Se recorre una vez cada sesión anotada (no una por día) y `CROSS JOIN` fija ese orden: sin
// estadísticas, el planificador prefiere recorrer `calls` entera y buscar cada fila en
// `dirty_keys`, en cada lote.
const DELETE_DAILY: &str = "DELETE FROM daily_stats
     WHERE (day, session_id) IN (SELECT day, session_id FROM dirty_keys)";

const INSERT_DAILY: &str = "INSERT INTO daily_stats
     SELECT c.day, c.session_id, c.model, COUNT(*), SUM(c.cost_usd), SUM(c.cache_savings_usd),
       SUM(c.input_tokens), SUM(c.output_tokens), SUM(c.cache_read), SUM(c.cache_write),
       SUM(c.has_price = 0), SUM(c.message_id IS s.first_call_id),
       SUM(CASE WHEN c.message_id IS s.first_call_id
                THEN c.input_tokens + c.cache_read + c.cache_write ELSE 0 END),
       MIN(c.ts), MAX(c.ts)
     FROM (SELECT DISTINCT session_id FROM dirty_keys) k
     CROSS JOIN calls c ON c.session_id = k.session_id
     JOIN sessions s ON s.id = c.session_id
     WHERE (c.session_id, c.day) IN (SELECT session_id, day FROM dirty_keys)
     GROUP BY c.day, c.session_id, c.model";

const DELETE_TOOLS: &str = "DELETE FROM tool_stats
     WHERE (day, session_id) IN (SELECT day, session_id FROM dirty_keys)";

const INSERT_TOOLS: &str = "INSERT INTO tool_stats
     SELECT t.day, t.session_id, t.tool, COUNT(*), SUM(t.is_error), MIN(t.ts)
     FROM (SELECT DISTINCT session_id FROM dirty_keys) k
     CROSS JOIN tool_calls t ON t.session_id = k.session_id
     WHERE (t.session_id, t.day) IN (SELECT session_id, day FROM dirty_keys)
     GROUP BY t.day, t.session_id, t.tool";

/// Calcula el día local (zona de la caché) de las filas de `table` que no lo tienen y anota sus
/// claves.
fn fill_days(conn: &Connection, table: &str, zone: &chrono_tz::Tz) -> Result<()> {
    let pending: Vec<(i64, String, i64)> = conn
        .prepare_cached(&format!(
            "SELECT rowid, session_id, ts FROM {table} WHERE day IS NULL"
        ))?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut update =
        conn.prepare_cached(&format!("UPDATE {table} SET day = ?1 WHERE rowid = ?2"))?;
    for (rowid, session, ts) in pending {
        let day = crate::tz::day_number(ts, zone);
        update.execute(params![day, rowid])?;
        mark_keys(conn, [(session.as_str(), day)])?;
    }
    Ok(())
}

/// Recalcula la primera llamada de las sesiones anotadas; si cambia, anota también los días de
/// la anterior y la nueva (sus agregados guardan el contexto de la primera llamada).
fn refresh_first_calls(conn: &Connection) -> Result<()> {
    let sessions: Vec<String> = conn
        .prepare_cached("SELECT DISTINCT session_id FROM dirty_keys")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for session in sessions {
        let old: Option<String> = conn
            .prepare_cached("SELECT first_call_id FROM sessions WHERE id = ?1")?
            .query_row([&session], |r| r.get(0))
            .optional()?
            .flatten();
        let new: Option<(String, Option<i64>)> = conn
            .prepare_cached(FIRST_CALL)?
            .query_row([&session], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        if old.as_deref() == new.as_ref().map(|(id, _)| id.as_str()) {
            continue;
        }
        conn.prepare_cached("UPDATE sessions SET first_call_id = ?2 WHERE id = ?1")?
            .execute(params![session, new.as_ref().map(|(id, _)| id)])?;
        if let Some((_, Some(day))) = new {
            mark_keys(conn, [(session.as_str(), day)])?;
        }
        if let Some(old) = old {
            let day: Option<Option<i64>> = conn
                .prepare_cached("SELECT day FROM calls WHERE message_id = ?1")?
                .query_row([old], |r| r.get(0))
                .optional()?;
            if let Some(Some(day)) = day {
                mark_keys(conn, [(session.as_str(), day)])?;
            }
        }
    }
    Ok(())
}

/// Recalcula lo pendiente. Va dentro de la transacción del llamador, antes de confirmarla.
pub fn refresh(conn: &Connection) -> Result<()> {
    let zone = crate::db::cache_zone(conn)?;
    fill_days(conn, "calls", &zone)?;
    conn.prepare_cached(MARK_PENDING_COSTS)?.execute([])?;
    conn.prepare_cached(REFRESH_COSTS)?.execute([])?;
    fill_days(conn, "tool_calls", &zone)?;
    refresh_first_calls(conn)?;
    for sql in [
        DELETE_DAILY,
        INSERT_DAILY,
        DELETE_TOOLS,
        INSERT_TOOLS,
        "DELETE FROM dirty_keys",
    ] {
        conn.prepare_cached(sql)?.execute([])?;
    }
    Ok(())
}

/// Lo que el escritor de la ingesta inserta, sumado por clave: llamadas por (día, sesión, modelo)
/// y herramientas por (día, sesión, herramienta). Sumar inserciones es exacto (las sumas se
/// suman, `MIN`/`MAX` se combinan), así que estas claves no se recalculan desde las llamadas, que
/// en una sesión larga son cada vez más: lo demás (filas que cambian o se borran, precios,
/// limpiezas, primera llamada) sigue yendo a `dirty_keys` y se rehace entero.
#[derive(Debug, Default)]
pub struct Deltas {
    calls: HashMap<(i64, String, String), CallSums>,
    tools: HashMap<(i64, String, String), ToolSums>,
    sessions: HashSet<String>,
}

#[derive(Debug, Default)]
struct CallSums {
    calls: i64,
    cost: f64,
    savings: f64,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    unpriced: i64,
    min_ts: i64,
    max_ts: i64,
}

#[derive(Debug, Default)]
struct ToolSums {
    uses: i64,
    errors: i64,
    min_ts: i64,
}

/// Una llamada nueva tal como queda en `calls`.
pub struct NewCall<'a> {
    pub day: i64,
    pub session: &'a str,
    pub model: &'a str,
    pub ts: i64,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub has_price: bool,
    pub cost: f64,
    pub savings: f64,
}

const ADD_DAILY: &str = "INSERT INTO daily_stats (day, session_id, model, calls, cost_usd,
       cache_savings_usd, input_tokens, output_tokens, cache_read, cache_write, unpriced_calls,
       first_calls, first_ctx_tokens, min_ts, max_ts)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, 0, ?12, ?13)
     ON CONFLICT(day, session_id, model) DO UPDATE SET
       calls = calls + excluded.calls, cost_usd = cost_usd + excluded.cost_usd,
       cache_savings_usd = cache_savings_usd + excluded.cache_savings_usd,
       input_tokens = input_tokens + excluded.input_tokens,
       output_tokens = output_tokens + excluded.output_tokens,
       cache_read = cache_read + excluded.cache_read, cache_write = cache_write + excluded.cache_write,
       unpriced_calls = unpriced_calls + excluded.unpriced_calls,
       min_ts = MIN(min_ts, excluded.min_ts), max_ts = MAX(max_ts, excluded.max_ts)";

const ADD_TOOLS: &str = "INSERT INTO tool_stats (day, session_id, tool, uses, errors, min_ts)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
     ON CONFLICT(day, session_id, tool) DO UPDATE SET
       uses = uses + excluded.uses, errors = errors + excluded.errors,
       min_ts = MIN(min_ts, excluded.min_ts)";

impl Deltas {
    pub fn call(&mut self, c: &NewCall) {
        let e = self
            .calls
            .entry((c.day, c.session.to_string(), c.model.to_string()))
            .or_insert_with(|| CallSums {
                min_ts: c.ts,
                max_ts: c.ts,
                ..Default::default()
            });
        e.calls += 1;
        e.cost += c.cost;
        e.savings += c.savings;
        e.input += c.input;
        e.output += c.output;
        e.cache_read += c.cache_read;
        e.cache_write += c.cache_write;
        e.unpriced += i64::from(!c.has_price);
        e.min_ts = e.min_ts.min(c.ts);
        e.max_ts = e.max_ts.max(c.ts);
        if !self.sessions.contains(c.session) {
            self.sessions.insert(c.session.to_string());
        }
    }

    /// `uses` herramientas nuevas y `errors` errores más (o menos) en la clave.
    pub fn tool(&mut self, day: i64, session: &str, tool: &str, ts: i64, uses: i64, errors: i64) {
        let e = self
            .tools
            .entry((day, session.to_string(), tool.to_string()))
            .or_insert_with(|| ToolSums {
                min_ts: ts,
                ..Default::default()
            });
        e.uses += uses;
        e.errors += errors;
        e.min_ts = e.min_ts.min(ts);
    }

    /// Suma lo acumulado a los agregados y anota las sesiones para revisar su primera llamada.
    pub fn apply(&mut self, conn: &Connection) -> Result<()> {
        let mut add = conn.prepare_cached(ADD_DAILY)?;
        for ((day, session, model), c) in self.calls.drain() {
            add.execute(params![
                day,
                session,
                model,
                c.calls,
                c.cost,
                c.savings,
                c.input,
                c.output,
                c.cache_read,
                c.cache_write,
                c.unpriced,
                c.min_ts,
                c.max_ts
            ])?;
        }
        let mut add = conn.prepare_cached(ADD_TOOLS)?;
        for ((day, session, tool), t) in self.tools.drain() {
            add.execute(params![day, session, tool, t.uses, t.errors, t.min_ts])?;
        }
        mark_keys(conn, self.sessions.iter().map(|s| (s.as_str(), 0)))?;
        self.sessions.clear();
        Ok(())
    }
}

/// Anota claves `(sesión, día)` cuyos derivados hay que rehacer desde las llamadas; el día 0 anota
/// solo la sesión (revisar su primera llamada).
pub fn mark_keys<'a>(
    conn: &Connection,
    keys: impl IntoIterator<Item = (&'a str, i64)>,
) -> Result<()> {
    let mut stmt = conn.prepare_cached("INSERT OR IGNORE INTO dirty_keys VALUES (?1, ?2)")?;
    for (session, day) in keys {
        stmt.execute(params![session, day])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use rusqlite::types::Value;

    /// Agregados diarios calculados desde cero sobre las llamadas: la referencia.
    const REBUILD_DAILY: &str = "SELECT c.day, c.session_id, c.model, COUNT(*), SUM(c.cost_usd),
         SUM(c.cache_savings_usd), SUM(c.input_tokens), SUM(c.output_tokens), SUM(c.cache_read),
         SUM(c.cache_write), SUM(c.has_price = 0), SUM(c.message_id IS s.first_call_id),
         SUM(CASE WHEN c.message_id IS s.first_call_id THEN c.input_tokens + c.cache_read + c.cache_write ELSE 0 END),
         MIN(c.ts), MAX(c.ts)
       FROM calls c JOIN sessions s ON s.id = c.session_id GROUP BY 1, 2, 3 ORDER BY 1, 2, 3";
    const STORED_DAILY: &str = "SELECT day, session_id, model, calls, cost_usd, cache_savings_usd,
         input_tokens, output_tokens, cache_read, cache_write, unpriced_calls, first_calls,
         first_ctx_tokens, min_ts, max_ts FROM daily_stats ORDER BY 1, 2, 3";
    const REBUILD_TOOLS: &str = "SELECT day, session_id, tool, COUNT(*), SUM(is_error), MIN(ts)
       FROM tool_calls GROUP BY 1, 2, 3 ORDER BY 1, 2, 3";
    const STORED_TOOLS: &str =
        "SELECT day, session_id, tool, uses, errors, min_ts FROM tool_stats ORDER BY 1, 2, 3";

    fn table(conn: &Connection, sql: &str) -> Vec<Vec<Value>> {
        let mut stmt = conn.prepare(sql).unwrap();
        let n = stmt.column_count();
        stmt.query_map([], |r| (0..n).map(|i| r.get::<_, Value>(i)).collect())
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// Iguales salvo reales con diferencias de redondeo (el orden de las sumas puede cambiar).
    fn assert_same(stored: &[Vec<Value>], rebuilt: &[Vec<Value>], what: &str) {
        assert_eq!(
            stored.len(),
            rebuilt.len(),
            "{what}: {stored:?}\n{rebuilt:?}"
        );
        for (a, b) in stored.iter().zip(rebuilt) {
            for (x, y) in a.iter().zip(b) {
                match (x, y) {
                    (Value::Real(x), Value::Real(y)) => {
                        assert!(
                            (x - y).abs() <= 1e-9 * x.abs().max(1.0),
                            "{what}: {a:?} {b:?}"
                        )
                    }
                    _ => assert_eq!(x, y, "{what}: {a:?} {b:?}"),
                }
            }
        }
    }

    fn assert_derived_up_to_date(conn: &Connection, step: &str) {
        refresh(conn).unwrap();
        assert_same(
            &table(conn, STORED_DAILY),
            &table(conn, REBUILD_DAILY),
            step,
        );
        assert_same(
            &table(conn, STORED_TOOLS),
            &table(conn, REBUILD_TOOLS),
            step,
        );
        let dirty: i64 = conn
            .query_row("SELECT COUNT(*) FROM dirty_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(dirty, 0, "{step}: quedan claves sucias");
    }

    /// Generador congruencial: secuencias aleatorias reproducibles sin dependencias.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self, n: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) % n
        }
    }

    const MODELS: [&str; 3] = ["claude-sonnet-4-5", "claude-haiku-4-5", "modelo-x"];
    /// 2026-10-24T12:00Z: los días aleatorios cruzan el cambio de hora del 25 en Madrid.
    const BASE_TS: i64 = 1_792_843_200_000;

    fn base() -> Connection {
        let conn = db::open_in_memory().unwrap();
        db::set_cache_tz(&conn, "Europe/Madrid").unwrap();
        conn.execute_batch(
            "INSERT INTO agents VALUES ('a','A','/',0);
             INSERT INTO sessions (id, agent_id, started_at, ended_at) VALUES
               ('s0','a',0,0), ('s1','a',0,0), ('s2','a',0,0);",
        )
        .unwrap();
        conn
    }

    /// Inserta como el escritor de la ingesta: coste y día calculados y la clave anotada.
    fn writer_insert(
        conn: &Connection,
        id: &str,
        session: &str,
        ts: i64,
        model: &str,
        tokens: i64,
    ) {
        let c = crate::providers::CallRec {
            message_id: id.into(),
            session_id: session.into(),
            ts,
            model: model.into(),
            input_tokens: tokens,
            output_tokens: tokens / 3,
            cache_read: tokens * 2,
            ..Default::default()
        };
        let cost = crate::pricing::PriceBook::load(conn).unwrap().cost(&c);
        let day = crate::tz::day_number(ts, &db::cache_zone(conn).unwrap());
        conn.execute(
            "INSERT INTO calls (message_id, session_id, ts, model, input_tokens, output_tokens,
               cache_read, has_price, cost_usd, cache_savings_usd, day)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                c.message_id,
                c.session_id,
                c.ts,
                c.model,
                c.input_tokens,
                c.output_tokens,
                c.cache_read,
                cost.has_price,
                cost.cost_usd,
                cost.cache_savings_usd,
                day
            ],
        )
        .unwrap();
        let mut deltas = Deltas::default();
        deltas.call(&NewCall {
            day,
            session,
            model,
            ts,
            input: c.input_tokens,
            output: c.output_tokens,
            cache_read: c.cache_read,
            cache_write: 0,
            has_price: cost.has_price,
            cost: cost.cost_usd,
            savings: cost.cache_savings_usd,
        });
        deltas.apply(conn).unwrap();
    }

    /// Herramienta nueva como la escribe el escritor: con día y sumada a sus agregados.
    fn writer_tool(conn: &Connection, id: &str, session: &str, ts: i64, tool: &str) {
        let day = crate::tz::day_number(ts, &db::cache_zone(conn).unwrap());
        conn.execute(
            "INSERT INTO tool_calls (call_id, session_id, ts, tool, day) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, session, ts, tool, day],
        )
        .unwrap();
        let mut deltas = Deltas::default();
        deltas.tool(day, session, tool, ts, 1, 0);
        deltas.apply(conn).unwrap();
    }

    /// Resultado de una herramienta como lo aplica el escritor: cambia `is_error` y suma la
    /// diferencia de errores a su clave.
    fn writer_result(conn: &Connection, id: &str, is_error: bool) {
        let old: Option<(String, Option<i64>, String, i64, bool)> = conn
            .query_row(
                "SELECT session_id, day, tool, ts, is_error FROM tool_calls WHERE call_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
            .unwrap();
        conn.execute(
            "UPDATE tool_calls SET is_error = ?2 WHERE call_id = ?1",
            rusqlite::params![id, is_error],
        )
        .unwrap();
        if let Some((session, Some(day), tool, ts, was)) = old {
            let mut deltas = Deltas::default();
            deltas.tool(
                day,
                &session,
                &tool,
                ts,
                0,
                i64::from(is_error) - i64::from(was),
            );
            deltas.apply(conn).unwrap();
        }
    }

    #[test]
    fn los_agregados_son_los_de_reconstruir_desde_cero_tras_cualquier_secuencia() {
        for seed in [1, 7, 42] {
            let conn = base();
            let mut rng = Lcg(seed);
            let mut calls = 0;
            let mut tools = 0;
            for step in 0..300 {
                let session = format!("s{}", rng.next(3));
                let ts = BASE_TS + rng.next(4 * 86_400) as i64 * 1000;
                let model = MODELS[rng.next(3) as usize];
                let op = rng.next(10);
                match op {
                    0..=2 => {
                        writer_insert(
                            &conn,
                            &format!("m{calls}"),
                            &session,
                            ts,
                            model,
                            rng.next(5000) as i64,
                        );
                        calls += 1;
                    }
                    3 => {
                        conn.execute(
                            "INSERT INTO calls (message_id, session_id, ts, model, input_tokens, is_sidechain)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            rusqlite::params![format!("m{calls}"), session, ts, model, rng.next(5000) as i64, rng.next(4) == 0],
                        )
                        .unwrap();
                        calls += 1;
                    }
                    4 => {
                        let id = format!("m{}", rng.next(calls.max(1)));
                        let sql = match rng.next(4) {
                            0 => "UPDATE calls SET ts = ?2 WHERE message_id = ?1",
                            1 => "UPDATE calls SET output_tokens = output_tokens + 1, cache_read = ?2 % 997 WHERE message_id = ?1",
                            2 => "UPDATE calls SET is_sidechain = 1 - is_sidechain, model = 'claude-haiku-4-5' WHERE message_id = ?1 AND ?2 > 0",
                            _ => "UPDATE calls SET ts = MIN(ts, ?2) WHERE message_id = ?1",
                        };
                        conn.execute(sql, rusqlite::params![id, ts]).unwrap();
                    }
                    5 => {
                        let id = format!("m{}", rng.next(calls.max(1)));
                        conn.execute("DELETE FROM calls WHERE message_id = ?1", [id])
                            .unwrap();
                    }
                    6 => {
                        let tool = ["Bash", "Read", "mcp__srv__x"][rng.next(3) as usize];
                        if rng.next(2) == 0 {
                            writer_tool(&conn, &format!("t{tools}"), &session, ts, tool);
                        } else {
                            conn.execute(
                                "INSERT INTO tool_calls (call_id, session_id, ts, tool, is_error) VALUES (?1, ?2, ?3, ?4, ?5)",
                                rusqlite::params![format!("t{tools}"), session, ts, tool, rng.next(3) == 0],
                            )
                            .unwrap();
                        }
                        tools += 1;
                        let id = format!("t{}", rng.next(tools));
                        writer_result(&conn, &id, rng.next(2) == 0);
                    }
                    7 => {
                        let id = format!("t{}", rng.next(tools.max(1)));
                        if rng.next(2) == 0 {
                            conn.execute("DELETE FROM tool_calls WHERE call_id = ?1", [id])
                                .unwrap();
                        } else {
                            conn.execute(
                                "UPDATE tool_calls SET ts = ?2 WHERE call_id = ?1",
                                rusqlite::params![id, ts],
                            )
                            .unwrap();
                        }
                    }
                    8 => {
                        conn.execute(
                            "INSERT OR REPLACE INTO prices (model, valid_from, input, output, cache_read)
                             VALUES (?1, ?2, ?3, 1.0, 0.1)",
                            rusqlite::params![model, ts, rng.next(10) as f64 + 0.5],
                        )
                        .unwrap();
                    }
                    _ => {
                        crate::pricing::apply_overrides(&conn, &[]).unwrap();
                    }
                }
                if rng.next(3) == 0 || step == 299 {
                    assert_derived_up_to_date(
                        &conn,
                        &format!("semilla {seed}, paso {step}, op {op}"),
                    );
                }
            }
        }
    }

    #[test]
    fn tras_escanear_los_fixtures_los_agregados_estan_al_dia() {
        use crate::providers::{claude_code::ClaudeCode, codex::Codex, Provider};
        let fx = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures");
        let providers: Vec<Box<dyn Provider>> = vec![
            Box::new(ClaudeCode::with_roots(vec![fx.join("claude_code")])),
            Box::new(Codex::with_roots(vec![fx.join("codex/sessions")])),
        ];
        let mut conn = db::open_in_memory().unwrap();
        crate::ingest::scan_all(&mut conn, &providers).unwrap();
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM daily_stats", [], |r| r.get(0))
            .unwrap();
        assert!(rows > 0);
        assert_derived_up_to_date(&conn, "fixtures");
    }

    #[test]
    fn el_refresco_no_recorre_las_tablas_grandes() {
        let conn = base();
        let mut scans = Vec::new();
        for sql in [
            REFRESH_COSTS,
            MARK_PENDING_COSTS,
            FIRST_CALL,
            DELETE_DAILY,
            INSERT_DAILY,
            DELETE_TOOLS,
            INSERT_TOOLS,
            "SELECT rowid, session_id, ts FROM calls WHERE day IS NULL",
            "SELECT rowid, session_id, ts FROM tool_calls WHERE day IS NULL",
        ] {
            scans.extend(db::trace::full_scans(&conn, sql));
        }
        assert!(scans.is_empty(), "{scans:#?}");
    }

    #[test]
    fn un_lote_que_toca_una_clave_solo_reescribe_esa_clave() {
        let conn = base();
        for (i, s) in ["s0", "s1", "s2"].iter().enumerate() {
            writer_insert(&conn, &format!("m{i}"), s, BASE_TS, MODELS[0], 100);
            writer_insert(
                &conn,
                &format!("n{i}"),
                s,
                BASE_TS + 3 * 86_400_000,
                MODELS[1],
                100,
            );
        }
        refresh(&conn).unwrap();
        conn.execute_batch(
            "CREATE TEMP TABLE rewritten (day INTEGER, session_id TEXT);
             CREATE TEMP TRIGGER spy_daily AFTER INSERT ON main.daily_stats
               BEGIN INSERT INTO rewritten VALUES (NEW.day, NEW.session_id); END;
             CREATE TEMP TRIGGER spy_tools AFTER INSERT ON main.tool_stats
               BEGIN INSERT INTO rewritten VALUES (NEW.day, NEW.session_id); END;",
        )
        .unwrap();
        writer_insert(&conn, "nueva", "s1", BASE_TS + 60_000, MODELS[2], 5);
        refresh(&conn).unwrap();
        let day = crate::tz::day_number(BASE_TS, &db::cache_zone(&conn).unwrap());
        assert_eq!(
            table(&conn, "SELECT day, session_id FROM rewritten ORDER BY 1, 2"),
            vec![vec![Value::Integer(day), Value::Text("s1".into())]],
            "solo la fila de ese día, sesión y modelo (una inserción suma, no recalcula)"
        );
        assert_derived_up_to_date(&conn, "tras la llamada nueva");
    }

    #[test]
    fn cambiar_la_zona_de_la_cache_rehace_los_agregados_con_los_dias_nuevos() {
        let conn = base();
        writer_insert(&conn, "m", "s0", 1_784_152_800_000, MODELS[0], 10);
        conn.execute(
            "INSERT INTO tool_calls (call_id, session_id, ts, tool) VALUES ('t', 's0', 1784152800000, 'Bash')",
            [],
        )
        .unwrap();
        assert_derived_up_to_date(&conn, "Madrid");
        db::set_cache_tz(&conn, "America/New_York").unwrap();
        assert_eq!(
            table(
                &conn,
                "SELECT DISTINCT day FROM daily_stats UNION SELECT day FROM tool_stats"
            ),
            vec![vec![Value::Integer(20260715)]]
        );
        assert_derived_up_to_date(&conn, "Nueva York");
    }
}
