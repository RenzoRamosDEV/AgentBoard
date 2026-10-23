//! Tabla de precios embebida (USD por millón de tokens) y normalización de nombres de modelo.
//!
//! Los precios de Claude salen de la tabla oficial de modelos (revisada el 2026-10-09);
//! escritura de caché = 1,25× entrada (5 min) y 2× entrada (1 h), salvo que la propia tabla
//! marque otro multiplicador de lectura de caché (p. ej. Opus 5.5 y Sonnet 5.5 van a 0,05×,
//! Fable 5.1 y Mythos 5.1 a 0,025×, el resto al 0,1× estándar). Los de OpenAI salen de su
//! página de precios oficial (misma revisión); los Codex ya no comparten precio con su
//! modelo base (gpt-5.3-codex cuesta más que gpt-5.3); desde GPT-5.6 la escritura en caché
//! se cobra (1,25× la entrada, sin distinción de 5 min/1 h), antes era gratis. Claude
//! Haiku 5.5 sube sus precios ×5 pasados los 100K tokens de prompt, y GPT-5.5 / GPT-5.6
//! Terra tienen un recargo pasados los 272K tokens de entrada; esta tabla no modela
//! precios por tramos de contexto. Los de Gemini son aproximados. Una fase posterior
//! los actualizará desde LiteLLM sin reimportar.

use crate::queries::Filter;
use crate::settings::PriceOverride;
use anyhow::Result;
use rusqlite::{params, params_from_iter, Connection};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// (modelo, entrada, salida, lectura de caché, escritura 5 min, escritura 1 h)
pub const DEFAULT_PRICES: &[(&str, f64, f64, f64, f64, f64)] = &[
    ("claude-fable-5-1", 10.0, 50.0, 0.25, 12.5, 20.0),
    ("claude-fable-5", 10.0, 50.0, 1.0, 12.5, 20.0),
    ("claude-mythos-5-1", 10.0, 50.0, 0.25, 12.5, 20.0),
    ("claude-mythos-5", 10.0, 50.0, 1.0, 12.5, 20.0),
    ("claude-opus-5-5", 4.0, 20.0, 0.20, 5.0, 8.0),
    ("claude-opus-5", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-8", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-7", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-6", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-5", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-1", 15.0, 75.0, 1.5, 18.75, 30.0),
    ("claude-opus-4", 15.0, 75.0, 1.5, 18.75, 30.0),
    ("claude-sonnet-5-5", 2.0, 10.0, 0.1, 2.5, 4.0),
    ("claude-sonnet-5", 2.0, 10.0, 0.2, 2.5, 4.0),
    ("claude-sonnet-4-6", 3.0, 15.0, 0.3, 3.75, 6.0),
    ("claude-sonnet-4-5", 3.0, 15.0, 0.3, 3.75, 6.0),
    ("claude-sonnet-4", 3.0, 15.0, 0.3, 3.75, 6.0),
    ("claude-haiku-5-5", 0.10, 0.50, 0.01, 0.125, 0.20),
    ("claude-haiku-4-5", 1.0, 5.0, 0.1, 1.25, 2.0),
    ("claude-3-5-haiku", 0.8, 4.0, 0.08, 1.0, 1.6),
    // Familia GPT-6 y GPT-5.6 (precio estándar, contexto corto; tabla oficial de octubre de 2026).
    // Desde GPT-5.6 la escritura en caché sí se cobra (1,25× la entrada); antes era gratis.
    ("gpt-6-astra", 10.0, 50.0, 1.0, 12.5, 0.0),
    ("gpt-6.1-sol", 2.0, 10.0, 0.10, 2.5, 0.0),
    ("gpt-6-sol", 2.0, 10.0, 0.2, 2.5, 0.0),
    ("gpt-6-luna", 0.10, 0.50, 0.01, 0.125, 0.0),
    ("gpt-5.6-sol", 4.0, 20.0, 0.40, 5.0, 0.0),
    ("gpt-5.6-terra", 2.0, 12.0, 0.20, 2.5, 0.0),
    ("gpt-5.6-luna", 0.20, 1.20, 0.02, 0.25, 0.0),
    ("gpt-5", 1.25, 10.0, 0.125, 0.0, 0.0),
    ("gpt-5-codex", 1.25, 10.0, 0.125, 0.0, 0.0),
    ("gpt-5.1", 1.25, 10.0, 0.125, 0.0, 0.0),
    ("gpt-5.1-codex", 1.25, 10.0, 0.125, 0.0, 0.0),
    ("gpt-5.1-codex-max", 1.25, 10.0, 0.125, 0.0, 0.0),
    ("gpt-5.1-codex-mini", 0.25, 2.0, 0.025, 0.0, 0.0),
    ("gpt-5.2-codex", 1.25, 10.0, 0.125, 0.0, 0.0),
    ("gpt-5.3-codex", 1.75, 14.0, 0.175, 0.0, 0.0),
    ("gpt-5.5-pro", 30.0, 180.0, 0.0, 0.0, 0.0),
    ("gpt-5.5", 5.0, 30.0, 0.5, 0.0, 0.0),
    ("gpt-5-mini", 0.25, 2.0, 0.025, 0.0, 0.0),
    ("gpt-5-nano", 0.05, 0.4, 0.005, 0.0, 0.0),
    ("o3", 2.0, 8.0, 0.5, 0.0, 0.0),
    ("o3-mini", 1.1, 4.4, 0.55, 0.0, 0.0),
    ("o4-mini", 1.1, 4.4, 0.275, 0.0, 0.0),
    ("codex-mini-latest", 1.5, 6.0, 0.375, 0.0, 0.0),
    ("gemini-2.5-pro", 1.25, 10.0, 0.31, 0.0, 0.0),
    ("gemini-2.5-flash", 0.30, 2.50, 0.075, 0.0, 0.0),
    ("gemini-2.5-flash-lite", 0.10, 0.40, 0.025, 0.0, 0.0),
    ("gemini-3-pro-preview", 2.0, 12.0, 0.20, 0.0, 0.0),
    ("gemini-3-pro", 2.0, 12.0, 0.20, 0.0, 0.0),
    ("gemini-3-flash", 0.50, 3.0, 0.05, 0.0, 0.0),
    ("gemini-3.8-flash", 0.75, 3.75, 0.075, 0.0, 0.0),
];

/// Precios que cambian en una fecha (epoch ms UTC): cada llamada usa el vigente en su día.
/// (modelo, desde, entrada, salida, lectura de caché, escritura 5 min, escritura 1 h)
pub const DATED_PRICES: &[(&str, i64, f64, f64, f64, f64, f64)] = &[
    // Gemini 3.8 Flash dobla su precio el 1 de enero de 2027.
    (
        "gemini-3.8-flash",
        1_798_761_600_000,
        1.50,
        7.50,
        0.15,
        0.0,
        0.0,
    ),
];

/// Inserta los precios por defecto (los de siempre y los que cambian en una fecha).
fn insert_defaults(tx: &Connection) -> Result<()> {
    let mut insert = tx.prepare(
        "INSERT INTO prices (model, valid_from, input, output, cache_read, cache_write, cache_write_1h)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for (model, i, o, cr, cw, cw1h) in DEFAULT_PRICES {
        insert.execute(params![model, 0, i, o, cr, cw, cw1h])?;
    }
    for (model, from, i, o, cr, cw, cw1h) in DATED_PRICES {
        insert.execute(params![model, from, i, o, cr, cw, cw1h])?;
    }
    Ok(())
}

/// Carga los precios por defecto si la tabla está vacía.
pub fn seed_if_empty(conn: &Connection) -> Result<()> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM prices", [], |r| r.get(0))?;
    if n > 0 {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    insert_defaults(&tx)?;
    tx.commit()?;
    Ok(())
}

/// Vuelve a los precios por defecto y aplica encima los del usuario: cada precio del usuario
/// sustituye a todos los del modelo (para todas las fechas). Los costes se recalculan solos
/// porque `call_costs` es una vista.
#[tracing::instrument(level = "debug", skip_all)]
pub fn apply_overrides(conn: &Connection, overrides: &[PriceOverride]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM prices", [])?;
    insert_defaults(&tx)?;
    {
        let mut insert = tx.prepare(
            "INSERT INTO prices (model, valid_from, input, output, cache_read, cache_write, cache_write_1h)
             VALUES (?1, 0, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut delete = tx.prepare("DELETE FROM prices WHERE model = ?1")?;
        for p in overrides {
            let model = normalize_model(&p.model);
            delete.execute(params![model])?;
            insert.execute(params![
                model,
                p.input,
                p.output,
                p.cache_read,
                p.cache_write,
                p.cache_write_1h
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceRow {
    pub model: String,
    /// `[entrada, salida, lectura caché, escritura 5 min, escritura 1 h]`; `None` si no hay precio.
    pub prices: Option<[f64; 5]>,
    /// `default`, `edited`, `reported` (coste del agente) o `missing`.
    pub source: String,
    /// Llamadas y coste en el filtro (0 si el modelo no se usó).
    pub calls: i64,
    pub cost_usd: f64,
}

/// Modelos usados en el filtro (sin precio primero, luego por llamadas) y después el resto con precio.
#[tracing::instrument(level = "debug", skip_all)]
pub fn list_prices(
    conn: &Connection,
    f: &Filter,
    overrides: &[PriceOverride],
) -> Result<Vec<PriceRow>> {
    let edited: HashSet<String> = overrides
        .iter()
        .map(|p| normalize_model(&p.model))
        .collect();
    let mut table: HashMap<String, [f64; 5]> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT model, input, output, cache_read, cache_write, cache_write_1h FROM prices p
         WHERE valid_from = (SELECT MAX(valid_from) FROM prices WHERE model = p.model)",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        table.insert(
            r.get(0)?,
            [r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?],
        );
    }
    let source = |model: &str, reported: bool| -> &'static str {
        if edited.contains(model) {
            "edited"
        } else if table.contains_key(model) {
            "default"
        } else if reported {
            "reported"
        } else {
            "missing"
        }
    };

    let (w, args) = f.sql("c.ts");
    let mut stmt = conn.prepare(&format!(
        "SELECT c.model, COUNT(*), SUM(c.cost_usd), MAX(c.cost_reported IS NOT NULL)
         FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w} GROUP BY 1"
    ))?;
    let mut used: Vec<PriceRow> = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            let model: String = r.get(0)?;
            let reported: bool = r.get(3)?;
            Ok(PriceRow {
                prices: table.get(&model).copied(),
                source: source(&model, reported).to_string(),
                calls: r.get(1)?,
                cost_usd: r.get(2)?,
                model,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    used.sort_by(|a, b| {
        (b.source == "missing")
            .cmp(&(a.source == "missing"))
            .then(b.calls.cmp(&a.calls))
            .then(a.model.cmp(&b.model))
    });
    let seen: HashSet<String> = used.iter().map(|r| r.model.clone()).collect();
    let mut rest: Vec<PriceRow> = table
        .iter()
        .filter(|(m, _)| !seen.contains(*m))
        .map(|(m, p)| PriceRow {
            model: m.clone(),
            prices: Some(*p),
            source: source(m, false).to_string(),
            calls: 0,
            cost_usd: 0.0,
        })
        .collect();
    rest.sort_by(|a, b| a.model.cmp(&b.model));
    used.extend(rest);
    Ok(used)
}

/// Nombre canónico para buscar precio: sin prefijo de proveedor, sin sufijo de fecha
/// (`-20251101`) ni marcas de contexto (`[1m]`, `-1m`), y versiones de Claude con guion
/// (`claude-opus-4.7` → `claude-opus-4-7`, como las escribe Copilot).
pub fn normalize_model(raw: &str) -> String {
    let mut m = raw.trim().to_ascii_lowercase();
    if let Some(pos) = m.rfind('/') {
        m = m[pos + 1..].to_string();
    }
    if let Some(pos) = m.find('[') {
        m.truncate(pos);
    }
    for suffix in ["-1m-internal", "-1m"] {
        if let Some(head) = m.strip_suffix(suffix) {
            m = head.to_string();
        }
    }
    if m.starts_with("claude-") {
        m = m.replace('.', "-");
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
        conn.query_row(
            "SELECT cost_usd FROM call_costs WHERE message_id = ?1",
            [id],
            |r| r.get(0),
        )
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
            .query_row(
                "SELECT has_price FROM call_costs WHERE message_id='b'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!has);
    }

    #[test]
    fn coste_reportado_cuando_no_hay_precio() {
        let conn = db::open_in_memory().unwrap();
        insert_call(&conn, "r", "big-pickle", 1000, 10);
        conn.execute(
            "UPDATE calls SET cost_reported = 0.0123 WHERE message_id = 'r'",
            [],
        )
        .unwrap();
        assert!((cost(&conn, "r") - 0.0123).abs() < 1e-12);
        let has: bool = conn
            .query_row(
                "SELECT has_price FROM call_costs WHERE message_id='r'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(has, "con coste reportado no cuenta como sin precio");
        // Con precio en la tabla, manda la tabla.
        insert_call(&conn, "t", "claude-sonnet-4-5", 1_000_000, 0);
        conn.execute(
            "UPDATE calls SET cost_reported = 99 WHERE message_id = 't'",
            [],
        )
        .unwrap();
        assert!((cost(&conn, "t") - 3.0).abs() < 1e-9);
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
            .query_row(
                "SELECT COUNT(*) FROM prices WHERE model LIKE 'claude-%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(n >= 10);
    }

    #[test]
    fn normaliza_nombres() {
        assert_eq!(
            normalize_model("claude-opus-4-5-20251101"),
            "claude-opus-4-5"
        );
        assert_eq!(
            normalize_model("anthropic/claude-sonnet-4-5"),
            "claude-sonnet-4-5"
        );
        assert_eq!(normalize_model("claude-opus-5-5[1m]"), "claude-opus-5-5");
        assert_eq!(normalize_model("gpt-5"), "gpt-5");
        assert_eq!(normalize_model("claude-opus-4.7"), "claude-opus-4-7");
        assert_eq!(normalize_model("claude-sonnet-4.5-1m"), "claude-sonnet-4-5");
        assert_eq!(normalize_model("gpt-5.3-codex"), "gpt-5.3-codex");
    }

    fn price(model: &str, input: f64) -> PriceOverride {
        PriceOverride {
            model: model.into(),
            input,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            cache_write_1h: 0.0,
        }
    }

    #[test]
    fn precio_del_usuario_completa_y_sustituye() {
        let conn = db::open_in_memory().unwrap();
        insert_call(&conn, "a", "kimi-k2", 1_000_000, 0);
        insert_call(&conn, "b", "claude-sonnet-4-5", 1_000_000, 0);
        assert_eq!(cost(&conn, "a"), 0.0);
        apply_overrides(
            &conn,
            &[price("kimi-k2", 1.0), price("claude-sonnet-4-5", 2.0)],
        )
        .unwrap();
        assert!(
            (cost(&conn, "a") - 1.0).abs() < 1e-9,
            "modelo sin precio completado"
        );
        assert!(
            (cost(&conn, "b") - 2.0).abs() < 1e-9,
            "sustituye al de por defecto"
        );
        // Restablecer: sin precios del usuario vuelve el de por defecto (3 USD) y kimi a 0.
        apply_overrides(&conn, &[]).unwrap();
        assert!((cost(&conn, "b") - 3.0).abs() < 1e-9);
        assert_eq!(cost(&conn, "a"), 0.0);
    }

    #[test]
    fn listado_de_precios_con_origen() {
        let conn = db::open_in_memory().unwrap();
        insert_call(&conn, "a", "kimi-k2", 10, 0);
        insert_call(&conn, "b", "claude-sonnet-4-5", 10, 0);
        insert_call(&conn, "c", "claude-sonnet-4-5", 10, 0);
        insert_call(&conn, "d", "big-pickle", 10, 0);
        conn.execute(
            "UPDATE calls SET cost_reported = 0.01 WHERE message_id = 'd'",
            [],
        )
        .unwrap();
        let overrides = [price("claude-haiku-4-5", 0.5)];
        apply_overrides(&conn, &overrides).unwrap();
        let rows = list_prices(&conn, &Filter::default(), &overrides).unwrap();
        let by = |m: &str| rows.iter().find(|r| r.model == m).unwrap();
        assert_eq!(rows[0].model, "kimi-k2", "sin precio, primero");
        assert_eq!(by("kimi-k2").source, "missing");
        assert_eq!(by("big-pickle").source, "reported");
        assert_eq!(
            (
                by("claude-sonnet-4-5").source.as_str(),
                by("claude-sonnet-4-5").calls
            ),
            ("default", 2)
        );
        assert_eq!(by("claude-haiku-4-5").source, "edited");
        assert_eq!(by("claude-haiku-4-5").prices.unwrap()[0], 0.5);
        assert!(
            rows.len() > 4,
            "incluye también los modelos con precio no usados"
        );
    }

    #[test]
    fn precios_de_gpt_6() {
        let conn = db::open_in_memory().unwrap();
        // 1M de entrada + 100K de salida en Astra: 10 + 5 = 15 USD.
        insert_call(&conn, "a", "gpt-6-astra", 1_000_000, 100_000);
        insert_call(&conn, "l", "gpt-6-luna", 1_000_000, 100_000);
        assert!((cost(&conn, "a") - 15.0).abs() < 1e-9);
        assert!((cost(&conn, "l") - 0.15).abs() < 1e-9);
    }

    #[test]
    fn gpt_6_cobra_la_escritura_en_cache() {
        let conn = db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT OR IGNORE INTO agents VALUES ('a','A','/',0);
             INSERT OR IGNORE INTO sessions (id, agent_id, started_at, ended_at) VALUES ('s','a',0,0);",
        )
        .unwrap();
        // 1M de tokens escritos en caché en Astra a 1,25× la entrada (10): 12.5 USD.
        conn.execute(
            "INSERT INTO calls (message_id, session_id, ts, model, cache_write)
             VALUES ('w', 's', 1000, 'gpt-6-astra', 1000000)",
            [],
        )
        .unwrap();
        assert!((cost(&conn, "w") - 12.5).abs() < 1e-9);
    }

    #[test]
    fn precios_de_la_familia_5_5() {
        let conn = db::open_in_memory().unwrap();
        // 1M de entrada + 100K de salida en Sonnet 5.5: 2 + 1 = 3 USD.
        insert_call(&conn, "s", "claude-sonnet-5-5", 1_000_000, 100_000);
        // 1M de entrada + 100K de salida en Haiku 5.5: 0.10 + 0.05 = 0.15 USD.
        insert_call(&conn, "h", "claude-haiku-5-5", 1_000_000, 100_000);
        // 1M de entrada + 100K de salida en GPT-5.5: 5 + 3 = 8 USD.
        insert_call(&conn, "g", "gpt-5.5", 1_000_000, 100_000);
        assert!((cost(&conn, "s") - 3.0).abs() < 1e-9);
        assert!((cost(&conn, "h") - 0.15).abs() < 1e-9);
        assert!((cost(&conn, "g") - 8.0).abs() < 1e-9);
    }

    #[test]
    fn multiplicador_de_cache_de_sonnet_5_5_y_mythos() {
        let conn = db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT OR IGNORE INTO agents VALUES ('a','A','/',0);
             INSERT OR IGNORE INTO sessions (id, agent_id, started_at, ended_at) VALUES ('s','a',0,0);",
        )
        .unwrap();
        let insert_cached = |id: &str, model: &str| {
            conn.execute(
                "INSERT INTO calls (message_id, session_id, ts, model, cache_read)
                 VALUES (?1, 's', 1000, ?2, 1000000)",
                params![id, model],
            )
            .unwrap();
        };
        // Sonnet 5.5 lee caché al 0,05× (0,10 USD/M), no al 0,1× de Sonnet 5 (0,20 USD/M).
        insert_cached("sonnet55", "claude-sonnet-5-5");
        assert!((cost(&conn, "sonnet55") - 0.10).abs() < 1e-9);
        // Mythos 5.1 lee caché al 0,025× (0,25 USD/M), igual que Fable 5.1.
        insert_cached("mythos51", "claude-mythos-5-1");
        assert!((cost(&conn, "mythos51") - 0.25).abs() < 1e-9);
        // Mythos 5 (sin "-1") lee caché al 0,1× estándar (1 USD/M), igual que Fable 5.
        insert_cached("mythos5", "claude-mythos-5");
        assert!((cost(&conn, "mythos5") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn gpt_5_3_codex_no_comparte_precio_con_su_base() {
        let conn = db::open_in_memory().unwrap();
        // 1M de entrada + 100K de salida: 1.75 + 1.40 = 3.15 USD (más caro que gpt-5, que daría 2.25).
        insert_call(&conn, "c", "gpt-5.3-codex", 1_000_000, 100_000);
        assert!((cost(&conn, "c") - 3.15).abs() < 1e-9);
    }

    #[test]
    fn precio_con_fecha_de_gemini_3_8_flash() {
        let conn = db::open_in_memory().unwrap();
        insert_call(&conn, "a", "gemini-3.8-flash", 1_000_000, 0);
        assert!((cost(&conn, "a") - 0.75).abs() < 1e-9, "2026: 0,75 USD/M");
        // La misma llamada el 2 de enero de 2027 cuesta el doble.
        conn.execute(
            "UPDATE calls SET ts = 1798848000000 WHERE message_id = 'a'",
            [],
        )
        .unwrap();
        assert!((cost(&conn, "a") - 1.50).abs() < 1e-9, "2027: 1,50 USD/M");
        // Y sobrevive a aplicar los precios del usuario.
        apply_overrides(&conn, &[]).unwrap();
        assert!((cost(&conn, "a") - 1.50).abs() < 1e-9);
    }
}
