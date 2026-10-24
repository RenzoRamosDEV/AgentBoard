//! Consultas agregadas que alimentan la UI. Casi todo es un GROUP BY sobre `call_costs`.

use anyhow::Result;
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// Filtro común a todas las vistas. Vacío = vista general.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Filter {
    /// Epoch ms inclusivo.
    pub from: Option<i64>,
    /// Epoch ms exclusivo.
    pub to: Option<i64>,
    /// Agentes incluidos; `None` = todos.
    pub agents: Option<Vec<String>>,
    /// Proyectos incluidos; `None` = todos.
    pub projects: Option<Vec<i64>>,
    /// Solo las sesiones sin proyecto (el cubo «(sin proyecto)» del detalle).
    pub no_project: bool,
}

impl Filter {
    /// `WHERE …` sobre `c` (call_costs o tabla con `ts`/`session_id`) unida a `s` (sessions).
    pub fn sql(&self, ts_col: &str) -> (String, Vec<Value>) {
        self.where_sql(ts_col, self.from, self.to)
    }

    /// `WHERE …` sobre un agregado diario (`day`) unido a `s`, con los días `[from, to)`.
    pub(crate) fn days_sql(
        &self,
        day_col: &str,
        from: Option<i64>,
        to: Option<i64>,
    ) -> (String, Vec<Value>) {
        self.where_sql(day_col, from, to)
    }

    fn where_sql(&self, col: &str, from: Option<i64>, to: Option<i64>) -> (String, Vec<Value>) {
        let mut conds = vec!["1=1".to_string()];
        let mut args: Vec<Value> = Vec::new();
        if let Some(from) = from {
            conds.push(format!("{col} >= ?"));
            args.push(from.into());
        }
        if let Some(to) = to {
            conds.push(format!("{col} < ?"));
            args.push(to.into());
        }
        if let Some(agents) = &self.agents {
            conds.push(format!("s.agent_id IN ({})", placeholders(agents.len())));
            args.extend(agents.iter().cloned().map(Value::from));
        }
        if self.no_project {
            conds.push("s.project_id IS NULL".to_string());
        }
        if let Some(projects) = &self.projects {
            // Un proyecto incluye todas las carpetas (worktrees) de su mismo repo.
            conds.push(format!(
                "s.project_id IN (SELECT id FROM projects WHERE repo_root IN
                   (SELECT repo_root FROM projects WHERE id IN ({})))",
                placeholders(projects.len())
            ));
            args.extend(projects.iter().map(|p| Value::from(*p)));
        }
        (conds.join(" AND "), args)
    }
}

/// De dónde lee una consulta: las llamadas o los agregados diarios de los días `[from, to)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Detail,
    Days { from: Option<i64>, to: Option<i64> },
}

impl Source {
    pub(crate) fn where_sql(&self, f: &Filter, col: &str) -> (String, Vec<Value>) {
        match *self {
            Source::Days { from, to } => f.days_sql(col, from, to),
            Source::Detail => f.sql(col),
        }
    }
}

/// Los agregados diarios sirven si el periodo son días locales completos de la zona de la caché
/// (sus extremos, si los hay, son medianoches de esa zona) y, en las consultas que agrupan por
/// días de `tz`, si esa zona es la de la caché. Si no, se responde desde las llamadas.
pub(crate) fn source(conn: &Connection, f: &Filter, tz: Option<&str>) -> Result<Source> {
    let cache = crate::db::cache_tz(conn)?;
    if cache.is_empty() || tz.is_some_and(|tz| tz != cache) {
        return Ok(Source::Detail);
    }
    let zone = crate::tz::parse(&cache);
    let day =
        |ts: i64| (crate::tz::day_start(ts, &zone) == ts).then(|| crate::tz::day_number(ts, &zone));
    match (f.from.map(day), f.to.map(day)) {
        (Some(None), _) | (_, Some(None)) => Ok(Source::Detail),
        (from, to) => Ok(Source::Days {
            from: from.flatten(),
            to: to.flatten(),
        }),
    }
}

fn placeholders(n: usize) -> String {
    if n == 0 {
        // IN () no es SQL válido; una lista vacía no casa con nada.
        return "NULL".into();
    }
    vec!["?"; n].join(",")
}

#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub cost_usd: f64,
    pub calls: i64,
    pub sessions: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    /// cache_read / (input + cache_read + cache_write), 0–1.
    pub cache_hit: f64,
    pub cache_savings_usd: f64,
    /// Coste de los últimos 60 min (agentes y proyectos filtrados, sin periodo), USD/h.
    pub burn_rate_usd_h: f64,
    pub first_ts: Option<i64>,
    pub last_ts: Option<i64>,
    pub unpriced_models: Vec<String>,
}

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;

fn ratio(num: i64, den: i64) -> f64 {
    if den > 0 {
        num as f64 / den as f64
    } else {
        0.0
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn summary(conn: &Connection, f: &Filter, now: i64) -> Result<Summary> {
    summary_from(conn, f, now, source(conn, f, None)?)
}

pub(crate) fn summary_from(
    conn: &Connection,
    f: &Filter,
    now: i64,
    src: Source,
) -> Result<Summary> {
    let (w, args) = src.where_sql(
        f,
        if src == Source::Detail {
            "c.ts"
        } else {
            "d.day"
        },
    );
    let (sql, unpriced) = match src {
        Source::Detail => (
            format!(
                "SELECT COALESCE(SUM(c.cost_usd),0), COUNT(*), COUNT(DISTINCT CASE WHEN s.is_subagent = 0 THEN c.session_id END),
                        COALESCE(SUM(c.input_tokens),0), COALESCE(SUM(c.output_tokens),0),
                        COALESCE(SUM(c.cache_read),0), COALESCE(SUM(c.cache_write),0),
                        COALESCE(SUM(c.cache_savings_usd),0), MIN(c.ts), MAX(c.ts)
                 FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w}"
            ),
            format!(
                "SELECT DISTINCT c.model FROM call_costs c JOIN sessions s ON s.id = c.session_id
                 WHERE {w} AND NOT c.has_price ORDER BY 1"
            ),
        ),
        Source::Days { .. } => (
            format!(
                "SELECT COALESCE(SUM(d.cost_usd),0), COALESCE(SUM(d.calls),0), COUNT(DISTINCT CASE WHEN s.is_subagent = 0 THEN d.session_id END),
                        COALESCE(SUM(d.input_tokens),0), COALESCE(SUM(d.output_tokens),0),
                        COALESCE(SUM(d.cache_read),0), COALESCE(SUM(d.cache_write),0),
                        COALESCE(SUM(d.cache_savings_usd),0), MIN(d.min_ts), MAX(d.max_ts)
                 FROM daily_stats d JOIN sessions s ON s.id = d.session_id WHERE {w}"
            ),
            format!(
                "SELECT DISTINCT d.model FROM daily_stats d JOIN sessions s ON s.id = d.session_id
                 WHERE {w} AND d.unpriced_calls > 0 ORDER BY 1"
            ),
        ),
    };
    let mut out = conn.query_row(&sql, params_from_iter(args.iter()), |r| {
        Ok(Summary {
            cost_usd: r.get(0)?,
            calls: r.get(1)?,
            sessions: r.get(2)?,
            input_tokens: r.get(3)?,
            output_tokens: r.get(4)?,
            cache_read: r.get(5)?,
            cache_write: r.get(6)?,
            cache_savings_usd: r.get(7)?,
            first_ts: r.get(8)?,
            last_ts: r.get(9)?,
            ..Default::default()
        })
    })?;
    out.cache_hit = ratio(
        out.cache_read,
        out.input_tokens + out.cache_read + out.cache_write,
    );

    let mut stmt = conn.prepare(&unpriced)?;
    out.unpriced_models = stmt
        .query_map(params_from_iter(args.iter()), |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;

    let recent = Filter {
        from: Some(now - HOUR_MS),
        to: None,
        ..f.clone()
    };
    let (w, args) = recent.sql("c.ts");
    out.burn_rate_usd_h = conn.query_row(
        &format!("SELECT COALESCE(SUM(c.cost_usd),0) FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    Ok(out)
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    /// Inicio del cubo en epoch ms UTC (medianoche u hora en punto locales).
    pub ts: i64,
    pub cost_usd: f64,
    pub calls: i64,
    pub sessions: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_write: i64,
}

/// Coste por día u hora local. `tz` es la zona IANA: cada fecha se agrupa con las reglas
/// de horario de SU momento (verano/invierno), no con el desfase de hoy.
#[tracing::instrument(level = "debug", skip_all, fields(bucket = %bucket))]
pub fn timeseries(conn: &Connection, f: &Filter, bucket: &str, tz: &str) -> Result<Vec<Point>> {
    let src = if bucket == "hour" {
        Source::Detail
    } else {
        source(conn, f, Some(tz))?
    };
    timeseries_from(conn, f, bucket, tz, src)
}

pub(crate) fn timeseries_from(
    conn: &Connection,
    f: &Filter,
    bucket: &str,
    tz: &str,
    src: Source,
) -> Result<Vec<Point>> {
    let zone = crate::tz::parse(tz);
    if let (Source::Days { .. }, false) = (src, bucket == "hour") {
        // El inicio del cubo es la medianoche local de cualquier llamada del día.
        let (w, args) = src.where_sql(f, "d.day");
        let sql = format!(
            "SELECT MIN(d.min_ts), SUM(d.cost_usd), SUM(d.calls),
                    COUNT(DISTINCT CASE WHEN s.is_subagent = 0 THEN d.session_id END),
                    SUM(d.input_tokens), SUM(d.output_tokens), SUM(d.cache_read), SUM(d.cache_write)
             FROM daily_stats d JOIN sessions s ON s.id = d.session_id WHERE {w}
             GROUP BY d.day ORDER BY d.day"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(args.iter()), |r| {
                Ok(Point {
                    ts: crate::tz::day_start(r.get(0)?, &zone),
                    cost_usd: r.get(1)?,
                    calls: r.get(2)?,
                    sessions: r.get(3)?,
                    input_tokens: r.get(4)?,
                    output_tokens: r.get(5)?,
                    cache_read: r.get(6)?,
                    cache_write: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        return Ok(rows);
    }
    let hour = bucket == "hour";
    let key = |ts: i64| {
        if hour {
            crate::tz::hour_start(ts, &zone)
        } else {
            crate::tz::day_start(ts, &zone)
        }
    };
    let (w, args) = f.sql("c.ts");
    let sql = format!(
        "SELECT c.ts, c.cost_usd, c.input_tokens, c.output_tokens, c.cache_read, c.cache_write,
                c.session_id, s.is_subagent
         FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(args.iter()))?;
    let mut acc: BTreeMap<i64, (Point, HashSet<String>)> = BTreeMap::new();
    while let Some(r) = rows.next()? {
        let b = key(r.get(0)?);
        let (p, sess) = acc.entry(b).or_insert_with(|| {
            (
                Point {
                    ts: b,
                    cost_usd: 0.0,
                    calls: 0,
                    sessions: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read: 0,
                    cache_write: 0,
                },
                HashSet::new(),
            )
        });
        p.cost_usd += r.get::<_, f64>(1)?;
        p.calls += 1;
        p.input_tokens += r.get::<_, i64>(2)?;
        p.output_tokens += r.get::<_, i64>(3)?;
        p.cache_read += r.get::<_, i64>(4)?;
        p.cache_write += r.get::<_, i64>(5)?;
        // Sesiones distintas del cubo, sin subagentes.
        if !r.get::<_, bool>(7)? {
            sess.insert(r.get(6)?);
        }
    }
    Ok(acc
        .into_values()
        .map(|(mut p, sess)| {
            p.sessions = sess.len() as i64;
            p
        })
        .collect())
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SeriesPoint {
    pub ts: i64,
    pub key: String,
    pub label: String,
    pub cost_usd: f64,
    pub calls: i64,
    pub output_tokens: i64,
}

/// Serie por día local y `agent | model | project | branch | tool` (esta última cuenta usos
/// de herramienta, sin coste). Misma agrupación con zona histórica que `timeseries`.
#[tracing::instrument(level = "debug", skip_all, fields(by = %by))]
pub fn timeseries_by(
    conn: &Connection,
    f: &Filter,
    by: &str,
    tz: &str,
) -> Result<Vec<SeriesPoint>> {
    timeseries_by_from(conn, f, by, tz, source(conn, f, Some(tz))?)
}

pub(crate) fn timeseries_by_from(
    conn: &Connection,
    f: &Filter,
    by: &str,
    tz: &str,
    src: Source,
) -> Result<Vec<SeriesPoint>> {
    if let Source::Days { .. } = src {
        return timeseries_by_days(conn, f, by, tz, src);
    }
    let zone = crate::tz::parse(tz);
    let mut acc: BTreeMap<(i64, String), SeriesPoint> = BTreeMap::new();
    let mut add = |ts: i64, key: String, label: String, cost: f64, out: i64| {
        let b = crate::tz::day_start(ts, &zone);
        let e = acc.entry((b, key.clone())).or_insert_with(|| SeriesPoint {
            ts: b,
            key,
            label: label.clone(),
            cost_usd: 0.0,
            calls: 0,
            output_tokens: 0,
        });
        if label < e.label {
            e.label = label; // determinista, como el MIN() del SQL de antes
        }
        e.cost_usd += cost;
        e.calls += 1;
        e.output_tokens += out;
    };
    if by == "tool" {
        let (w, args) = f.sql("t.ts");
        let sql = format!(
            r"SELECT t.ts, t.tool FROM tool_calls t JOIN sessions s ON s.id = t.session_id
              WHERE {w} AND t.tool NOT LIKE 'mcp\_\_%' ESCAPE '\'"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(args.iter()))?;
        while let Some(r) = rows.next()? {
            let tool: String = r.get(1)?;
            add(r.get(0)?, tool.clone(), tool, 0.0, 0);
        }
    } else {
        let (key, label, join) = match by {
            "agent" => (
                "s.agent_id",
                "COALESCE(a.name, s.agent_id)",
                "LEFT JOIN agents a ON a.id = s.agent_id",
            ),
            "model" => ("c.model", "c.model", ""),
            "project" => (
                "COALESCE(p.repo_root, '')",
                "COALESCE(p.name, '(sin proyecto)')",
                "LEFT JOIN projects p ON p.id = s.project_id",
            ),
            "branch" => (
                "COALESCE(s.git_branch, '')",
                "COALESCE(s.git_branch, '(sin rama)')",
                "",
            ),
            other => anyhow::bail!("serie desconocida: {other}"),
        };
        let (w, args) = f.sql("c.ts");
        let sql = format!(
            "SELECT c.ts, {key}, {label}, c.cost_usd, c.output_tokens
             FROM call_costs c JOIN sessions s ON s.id = c.session_id {join} WHERE {w}"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(args.iter()))?;
        while let Some(r) = rows.next()? {
            add(r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?);
        }
    }
    // Como antes: por cubo y, dentro, lo más gastado (o usado) primero.
    let mut out: Vec<SeriesPoint> = acc.into_values().collect();
    sort_series(&mut out);
    Ok(out)
}

/// Clave, etiqueta y uniones de cada agrupación de las llamadas, sobre `c` (llamadas) o `d`
/// (agregados diarios) unidos a `s` (sesiones).
fn call_grouping(by: &str, t: &str) -> Option<(String, String, &'static str)> {
    Some(match by {
        "agent" => (
            "s.agent_id".into(),
            "COALESCE(a.name, s.agent_id)".into(),
            "LEFT JOIN agents a ON a.id = s.agent_id",
        ),
        "model" => (format!("{t}.model"), format!("{t}.model"), ""),
        "project" => (
            "COALESCE(p.repo_root, '')".into(),
            "COALESCE(p.name, '(sin proyecto)')".into(),
            "LEFT JOIN projects p ON p.id = s.project_id",
        ),
        "branch" => (
            "COALESCE(s.git_branch, '')".into(),
            "COALESCE(s.git_branch, '(sin rama)')".into(),
            "",
        ),
        _ => return None,
    })
}

/// Lo más gastado (o usado) primero dentro de cada cubo, como hace la ruta de detalle.
fn sort_series(out: &mut [SeriesPoint]) {
    out.sort_by(|a, b| {
        a.ts.cmp(&b.ts)
            .then(b.cost_usd.total_cmp(&a.cost_usd))
            .then(b.calls.cmp(&a.calls))
            .then(a.key.cmp(&b.key))
    });
}

fn timeseries_by_days(
    conn: &Connection,
    f: &Filter,
    by: &str,
    tz: &str,
    src: Source,
) -> Result<Vec<SeriesPoint>> {
    let zone = crate::tz::parse(tz);
    let sql = if by == "tool" {
        let (w, _) = src.where_sql(f, "t.day");
        format!(
            r"SELECT MIN(t.min_ts), t.tool, t.tool, 0.0, SUM(t.uses), 0
              FROM tool_stats t JOIN sessions s ON s.id = t.session_id
              WHERE {w} AND t.tool NOT LIKE 'mcp\_\_%' ESCAPE '\' GROUP BY t.day, t.tool"
        )
    } else {
        let Some((key, label, join)) = call_grouping(by, "d") else {
            anyhow::bail!("serie desconocida: {by}");
        };
        let (w, _) = src.where_sql(f, "d.day");
        format!(
            "SELECT MIN(d.min_ts), {key}, MIN({label}), SUM(d.cost_usd), SUM(d.calls), SUM(d.output_tokens)
             FROM daily_stats d JOIN sessions s ON s.id = d.session_id {join} WHERE {w}
             GROUP BY d.day, {key}"
        )
    };
    let (_, args) = src.where_sql(f, "x");
    let mut stmt = conn.prepare(&sql)?;
    let mut out: Vec<SeriesPoint> = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(SeriesPoint {
                ts: crate::tz::day_start(r.get(0)?, &zone),
                key: r.get(1)?,
                label: r.get(2)?,
                cost_usd: r.get(3)?,
                calls: r.get(4)?,
                output_tokens: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    sort_series(&mut out);
    Ok(out)
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BreakdownRow {
    pub key: String,
    pub label: String,
    pub cost_usd: f64,
    pub calls: i64,
    pub errors: i64,
    pub cache_hit: f64,
    pub has_price: bool,
    pub sessions: i64,
    /// Media de tokens de contexto de la primera llamada de cada sesión.
    pub overhead_tokens: f64,
    /// Agente al que pertenece la fila, en los desgloses que mezclan agentes (tipos de subagente).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

impl BreakdownRow {
    pub fn simple(key: impl Into<String>, calls: i64, errors: i64, cost_usd: f64) -> Self {
        let key = key.into();
        Self {
            label: key.clone(),
            key,
            cost_usd,
            calls,
            errors,
            cache_hit: 0.0,
            has_price: true,
            sessions: 0,
            overhead_tokens: 0.0,
            agent: None,
        }
    }
}

/// Coste y uso agrupado por `agent | project | branch | model | activity | tool | command | skill | mcp | agent_type`.
#[tracing::instrument(level = "debug", skip_all, fields(by = %by))]
pub fn breakdown(conn: &Connection, f: &Filter, by: &str) -> Result<Vec<BreakdownRow>> {
    let src = match by {
        "agent" | "project" | "branch" | "model" | "tool" | "mcp" => source(conn, f, None)?,
        _ => Source::Detail,
    };
    breakdown_from(conn, f, by, src)
}

pub(crate) fn breakdown_from(
    conn: &Connection,
    f: &Filter,
    by: &str,
    src: Source,
) -> Result<Vec<BreakdownRow>> {
    match by {
        "command" => return crate::insights::shell_commands(conn, f),
        "skill" => return crate::insights::skills_and_agents(conn, f),
        "mcp" => return crate::insights::mcp_servers_from(conn, f, src),
        "agent_type" => return crate::insights::agent_types(conn, f),
        _ => {}
    }
    let days = src != Source::Detail && by != "activity";
    let (w, args) = match (days, by == "tool") {
        (true, true) => src.where_sql(f, "t.day"),
        (true, false) => src.where_sql(f, "d.day"),
        (false, true) => f.sql("t.ts"),
        (false, false) => f.sql("c.ts"),
    };
    let t = if days { "d" } else { "c" };
    // `s.first_call_id` = primera llamada principal de la sesión, para el overhead de contexto.
    let calls = |key: &str, label: &str, join: &str| {
        if days {
            return format!(
                "SELECT {key}, {label}, SUM(d.cost_usd), SUM(d.calls), 0,
                        SUM(d.cache_read), SUM(d.input_tokens + d.cache_read + d.cache_write), MIN(d.unpriced_calls = 0),
                        COUNT(DISTINCT CASE WHEN s.is_subagent = 0 THEN d.session_id END),
                        COALESCE(CAST(SUM(d.first_ctx_tokens) AS REAL) / NULLIF(SUM(d.first_calls), 0), 0)
                 FROM daily_stats d JOIN sessions s ON s.id = d.session_id {join}
                 WHERE {w} GROUP BY 1 ORDER BY 3 DESC, 4 DESC"
            );
        }
        format!(
            "SELECT {key}, {label}, SUM(c.cost_usd), COUNT(*), 0,
                    SUM(c.cache_read), SUM(c.input_tokens + c.cache_read + c.cache_write), MIN(c.has_price),
                    COUNT(DISTINCT CASE WHEN s.is_subagent = 0 THEN c.session_id END),
                    COALESCE(AVG(CASE WHEN c.message_id = s.first_call_id THEN c.input_tokens + c.cache_read + c.cache_write END), 0)
             FROM call_costs c JOIN sessions s ON s.id = c.session_id {join}
             WHERE {w} GROUP BY 1 ORDER BY 3 DESC, 4 DESC"
        )
    };
    let tools = |key: &str, extra: &str| {
        if days {
            return format!(
                "SELECT {key}, {key}, 0.0, SUM(t.uses), SUM(t.errors), 0, 0, 1, 0, 0.0
                 FROM tool_stats t JOIN sessions s ON s.id = t.session_id
                 WHERE {w} {extra} GROUP BY 1 ORDER BY 4 DESC LIMIT 50"
            );
        }
        format!(
            "SELECT {key}, {key}, 0.0, COUNT(*), SUM(t.is_error), 0, 0, 1, 0, 0.0
             FROM tool_calls t JOIN sessions s ON s.id = t.session_id
             WHERE {w} {extra} GROUP BY 1 ORDER BY 4 DESC LIMIT 50"
        )
    };
    let model = format!("{t}.model");
    let sql = match by {
        "agent" => calls(
            "s.agent_id",
            "COALESCE(a.name, s.agent_id)",
            "LEFT JOIN agents a ON a.id = s.agent_id",
        ),
        "project" => calls(
            "COALESCE(p.repo_root, '')",
            "COALESCE(MIN(p.name), '(sin proyecto)')",
            "LEFT JOIN projects p ON p.id = s.project_id",
        ),
        "branch" => calls(
            "COALESCE(s.git_branch, '')",
            "COALESCE(s.git_branch, '(sin rama)')",
            "",
        ),
        "model" => calls(&model, &model, ""),
        "activity" => calls(
            "COALESCE(c.activity, 'conversation')",
            "COALESCE(c.activity, 'conversation')",
            "",
        ),
        // Core tools: las nativas, sin las de servidores MCP.
        "tool" => tools("t.tool", r"AND t.tool NOT LIKE 'mcp\_\_%' ESCAPE '\'"),
        other => anyhow::bail!("agrupación desconocida: {other}"),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            let key: Option<String> = r.get(0)?;
            let label: Option<String> = r.get(1)?;
            Ok(BreakdownRow {
                key: key.clone().unwrap_or_default(),
                label: label.or(key).unwrap_or_default(),
                cost_usd: r.get(2)?,
                calls: r.get(3)?,
                errors: r.get(4)?,
                cache_hit: ratio(r.get(5)?, r.get(6)?),
                has_price: r.get(7)?,
                sessions: r.get(8)?,
                overhead_tokens: r.get(9)?,
                agent: None,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRow {
    pub id: String,
    pub name: String,
    pub log_root: String,
    pub cost_usd: f64,
    pub calls: i64,
}

/// Agentes detectados con su coste en el periodo (sin filtrar por agente ni proyecto).
#[tracing::instrument(level = "debug", skip_all)]
pub fn list_agents(conn: &Connection, f: &Filter) -> Result<Vec<AgentRow>> {
    list_agents_from(conn, f, source(conn, f, None)?)
}

/// Coste y llamadas por `group` (columna de `s`) en el filtro, desde las llamadas o los agregados.
fn cost_by(f: &Filter, src: Source, group: &str) -> (String, Vec<Value>) {
    let (w, args) = match src {
        Source::Detail => f.sql("c.ts"),
        Source::Days { .. } => src.where_sql(f, "c.day"),
    };
    let sql = match src {
        Source::Detail => format!(
            "SELECT s.{group} AS id, SUM(c.cost_usd) AS cost, COUNT(*) AS n
             FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w} GROUP BY 1"
        ),
        Source::Days { .. } => format!(
            "SELECT s.{group} AS id, SUM(c.cost_usd) AS cost, SUM(c.calls) AS n
             FROM daily_stats c JOIN sessions s ON s.id = c.session_id WHERE {w} GROUP BY 1"
        ),
    };
    (sql, args)
}

pub(crate) fn list_agents_from(
    conn: &Connection,
    f: &Filter,
    src: Source,
) -> Result<Vec<AgentRow>> {
    let period = Filter {
        from: f.from,
        to: f.to,
        ..Default::default()
    };
    let (inner, args) = cost_by(&period, src, "agent_id");
    let sql = format!(
        "SELECT a.id, a.name, a.log_root, COALESCE(x.cost, 0), COALESCE(x.n, 0)
         FROM agents a LEFT JOIN ({inner}) x ON x.id = a.id
         ORDER BY 4 DESC, 5 DESC, a.name"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(AgentRow {
                id: r.get(0)?,
                name: r.get(1)?,
                log_root: r.get(2)?,
                cost_usd: r.get(3)?,
                calls: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    pub id: i64,
    pub name: String,
    pub cwd: String,
    pub cost_usd: f64,
    pub calls: i64,
}

/// Proyectos con su coste en el periodo y agentes filtrados (sin filtrar por proyecto).
#[tracing::instrument(level = "debug", skip_all)]
pub fn list_projects(conn: &Connection, f: &Filter) -> Result<Vec<ProjectRow>> {
    list_projects_from(conn, f, source(conn, f, None)?)
}

pub(crate) fn list_projects_from(
    conn: &Connection,
    f: &Filter,
    src: Source,
) -> Result<Vec<ProjectRow>> {
    let scope = Filter {
        projects: None,
        ..f.clone()
    };
    let (inner, args) = cost_by(&scope, src, "project_id");
    // Varias carpetas (worktrees) pueden compartir repo: se agrupan por repo_root.
    let sql = format!(
        "SELECT MIN(p.id), p.name, p.repo_root, COALESCE(SUM(x.cost), 0), COALESCE(SUM(x.n), 0)
         FROM projects p LEFT JOIN ({inner}) x ON x.id = p.id
         WHERE EXISTS (SELECT 1 FROM sessions s WHERE s.project_id = p.id)
         GROUP BY p.repo_root ORDER BY 4 DESC, 5 DESC, p.name"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(ProjectRow {
                id: r.get(0)?,
                name: r.get(1)?,
                cwd: r.get(2)?,
                cost_usd: r.get(3)?,
                calls: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DataInfo {
    pub first_ts: Option<i64>,
    pub calls: i64,
    pub watched_files: i64,
    pub last_scan: Option<i64>,
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn data_info(conn: &Connection) -> Result<DataInfo> {
    conn.query_row(
        "SELECT MIN(ts), COUNT(*), (SELECT COUNT(*) FROM file_state), (SELECT MAX(last_scan) FROM file_state) FROM calls",
        [],
        |r| Ok(DataInfo { first_ts: r.get(0)?, calls: r.get(1)?, watched_files: r.get(2)?, last_scan: r.get(3)? }),
    )
    .map_err(Into::into)
}

#[cfg(test)]
pub(crate) mod testdata {
    use rusqlite::{params, Connection};

    /// Dos agentes, dos proyectos, tres sesiones.
    pub fn seed(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO agents VALUES ('claude-code','Claude Code','/c',0),('codex','Codex','/x',0);
             INSERT INTO projects (id,name,cwd,repo_root) VALUES (1,'web','/w','/w'),(2,'api','/a','/a');
             INSERT INTO sessions (id,agent_id,project_id,git_branch,started_at,ended_at) VALUES
               ('s1','claude-code',1,'main',1000,5000),('s2','claude-code',2,'feat',2000,3000),('s3','codex',1,'main',9000,9500);",
        )
        .unwrap();
        for (id, s, ts, model, i, o) in [
            ("m1", "s1", 1000, "claude-sonnet-4-5", 1_000_000, 0),
            ("m2", "s1", 4000, "claude-sonnet-4-5", 0, 100_000),
            ("m3", "s2", 2000, "claude-haiku-4-5", 1_000_000, 0),
            ("m4", "s3", 9000, "modelo-x", 5, 5),
        ] {
            conn.execute(
                "INSERT INTO calls (message_id,session_id,ts,model,input_tokens,output_tokens) VALUES (?1,?2,?3,?4,?5,?6)",
                params![id, s, ts, model, i, o],
            )
            .unwrap();
        }
        crate::derived::refresh(conn).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn resumen_general() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let s = summary(&conn, &Filter::default(), 0).unwrap();
        assert_eq!((s.calls, s.sessions), (4, 3));
        // 3 + 1.5 (sonnet) + 1 (haiku) + 0 (sin precio)
        assert!((s.cost_usd - 5.5).abs() < 1e-9);
        assert_eq!(s.unpriced_models, vec!["modelo-x".to_string()]);
    }

    #[test]
    fn filtros_por_agente_proyecto_y_periodo() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let by_agent = summary(
            &conn,
            &Filter {
                agents: Some(vec!["codex".into()]),
                ..Default::default()
            },
            0,
        )
        .unwrap();
        assert_eq!(by_agent.calls, 1);
        let by_project = summary(
            &conn,
            &Filter {
                projects: Some(vec![2]),
                ..Default::default()
            },
            0,
        )
        .unwrap();
        assert!((by_project.cost_usd - 1.0).abs() < 1e-9);
        let by_time = summary(
            &conn,
            &Filter {
                from: Some(1500),
                to: Some(5000),
                ..Default::default()
            },
            0,
        )
        .unwrap();
        assert_eq!(by_time.calls, 2);
        let none = summary(
            &conn,
            &Filter {
                agents: Some(vec![]),
                ..Default::default()
            },
            0,
        )
        .unwrap();
        assert_eq!(none.calls, 0);
    }

    #[test]
    fn cache_hit_y_burn_rate() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        conn.execute(
            "INSERT INTO calls (message_id,session_id,ts,model,input_tokens,cache_read,cache_write) VALUES ('c','s1',100000,'claude-sonnet-4-5',100,800,100)",
            [],
        )
        .unwrap();
        crate::derived::refresh(&conn).unwrap();
        let s = summary(
            &conn,
            &Filter {
                from: Some(100000),
                ..Default::default()
            },
            100000 + 10 * 60_000,
        )
        .unwrap();
        assert!((s.cache_hit - 0.8).abs() < 1e-9);
        assert!(s.burn_rate_usd_h > 0.0);
        // ahorro: 800 × (3 − 0,3) / 1e6
        assert!((s.cache_savings_usd - 800.0 * 2.7 / 1e6).abs() < 1e-12);
        let idle = summary(&conn, &Filter::default(), 100000 + 2 * HOUR_MS).unwrap();
        assert_eq!(idle.burn_rate_usd_h, 0.0);
    }

    #[test]
    fn serie_diaria_agrupa_cada_fecha_con_su_horario() {
        use chrono::{TimeZone, Utc};
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        // Enero es UTC+1 en Madrid y julio UTC+2: ambas llamadas son las 00:30 locales del 16.
        let enero = Utc
            .with_ymd_and_hms(2026, 1, 15, 23, 30, 0)
            .unwrap()
            .timestamp_millis();
        let julio = Utc
            .with_ymd_and_hms(2026, 7, 15, 22, 30, 0)
            .unwrap()
            .timestamp_millis();
        for (id, ts) in [("d1", enero), ("d2", julio)] {
            conn.execute(
                "INSERT INTO calls (message_id,session_id,ts,model) VALUES (?1,'s1',?2,'claude-sonnet-4-5')",
                rusqlite::params![id, ts],
            )
            .unwrap();
        }
        let f = Filter {
            from: Some(enero),
            ..Default::default()
        };
        let local = timeseries(&conn, &f, "day", "Europe/Madrid").unwrap();
        // Cada día empieza en SU medianoche local expresada en UTC (23:00Z y 22:00Z del 15).
        assert_eq!(
            local.iter().map(|p| p.ts).collect::<Vec<_>>(),
            vec![enero - 1_800_000, julio - 1_800_000]
        );
        // En UTC caen en el propio día 15.
        let utc = timeseries(&conn, &f, "day", "UTC").unwrap();
        assert_eq!(utc.len(), 2);
        assert_eq!(utc[0].ts, enero - enero.rem_euclid(86_400_000));
    }

    #[test]
    fn desglose_por_modelo_proyecto_y_rama() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let models = breakdown(&conn, &Filter::default(), "model").unwrap();
        assert_eq!(models[0].key, "claude-sonnet-4-5");
        assert!(models.iter().any(|m| m.key == "modelo-x" && !m.has_price));
        let projects = breakdown(&conn, &Filter::default(), "project").unwrap();
        assert_eq!(projects[0].label, "web");
        let branches = breakdown(
            &conn,
            &Filter {
                projects: Some(vec![1]),
                ..Default::default()
            },
            "branch",
        )
        .unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].label, "main");
        assert!(breakdown(&conn, &Filter::default(), "nada").is_err());
    }

    #[test]
    fn serie_diaria_por_agente() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let rows = timeseries_by(&conn, &Filter::default(), "agent", "UTC").unwrap();
        assert_eq!(rows.len(), 2, "un día, dos agentes");
        assert_eq!((rows[0].label.as_str(), rows[0].calls), ("Claude Code", 3));
        let day = timeseries(&conn, &Filter::default(), "day", "UTC").unwrap();
        assert_eq!(
            (day[0].sessions, day[0].input_tokens, day[0].output_tokens),
            (3, 2_000_005, 100_005)
        );
        assert!(timeseries_by(&conn, &Filter::default(), "nada", "UTC").is_err());
        let projects = timeseries_by(&conn, &Filter::default(), "project", "UTC").unwrap();
        assert_eq!(
            projects
                .iter()
                .map(|p| p.label.as_str())
                .collect::<Vec<_>>(),
            vec!["web", "api"]
        );
        conn.execute("INSERT INTO tool_calls (call_id,session_id,ts,tool) VALUES ('x1','s1',1000,'Bash'),('x2','s1',1000,'Bash'),('x3','s1',1000,'mcp__srv__t')", []).unwrap();
        crate::derived::refresh(&conn).unwrap();
        let tools = timeseries_by(&conn, &Filter::default(), "tool", "UTC").unwrap();
        assert_eq!(
            tools
                .iter()
                .map(|p| (p.label.as_str(), p.calls))
                .collect::<Vec<_>>(),
            vec![("Bash", 2)],
            "sin MCP"
        );
    }

    #[test]
    fn desglose_por_agente() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let rows = breakdown(&conn, &Filter::default(), "agent").unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| (r.label.as_str(), r.calls, r.sessions))
                .collect::<Vec<_>>(),
            vec![("Claude Code", 3, 2), ("Codex", 1, 1)]
        );
    }

    #[test]
    fn comandos_agrupados_por_primera_palabra() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        for (i, (tool, target, err)) in [
            ("Bash", "git status", 0),
            ("Bash", "git diff", 1),
            ("Bash", "npm test", 0),
            ("Read", "/a.rs", 0),
        ]
        .iter()
        .enumerate()
        {
            conn.execute(
                "INSERT INTO tool_calls (call_id,session_id,ts,tool,target,is_error) VALUES (?1,'s1',1000,?2,?3,?4)",
                rusqlite::params![format!("t{i}"), tool, target, err],
            )
            .unwrap();
        }
        crate::derived::refresh(&conn).unwrap();
        let cmds = breakdown(&conn, &Filter::default(), "command").unwrap();
        assert_eq!(cmds.len(), 2);
        assert_eq!(
            (cmds[0].key.as_str(), cmds[0].calls, cmds[0].errors),
            ("git", 2, 1)
        );
        assert_eq!((cmds[1].key.as_str(), cmds[1].calls), ("npm", 1));
        let tools = breakdown(&conn, &Filter::default(), "tool").unwrap();
        assert_eq!((tools[0].key.as_str(), tools[0].calls), ("Bash", 3));
    }

    #[test]
    fn listas_de_agentes_y_proyectos() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let agents = list_agents(
            &conn,
            &Filter {
                agents: Some(vec![]),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(agents.len(), 2, "la lista ignora el filtro de agentes");
        assert_eq!(agents[0].id, "claude-code");
        let projects = list_projects(
            &conn,
            &Filter {
                from: Some(8000),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!((projects[0].name.as_str(), projects[0].calls), ("web", 1));
        assert_eq!(projects[1].calls, 0);
    }

    #[test]
    fn worktrees_cuentan_en_su_proyecto() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        conn.execute_batch(
            "INSERT INTO projects (id,name,cwd,repo_root) VALUES (3,'web','/w/.claude/worktrees/x','/w');
             INSERT INTO sessions (id,agent_id,project_id,started_at,ended_at) VALUES ('s4','claude-code',3,0,0);
             INSERT INTO calls (message_id,session_id,ts,model,input_tokens) VALUES ('m5','s4',1,'claude-sonnet-4-5',1000000);",
        )
        .unwrap();
        crate::derived::refresh(&conn).unwrap();
        let web = summary(
            &conn,
            &Filter {
                projects: Some(vec![1]),
                ..Default::default()
            },
            0,
        )
        .unwrap();
        assert_eq!(web.calls, 4, "3 de /w + 1 del worktree");
        let rows = breakdown(&conn, &Filter::default(), "project").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(list_projects(&conn, &Filter::default()).unwrap().len(), 2);
    }

    fn first_call(conn: &Connection, session: &str) -> Option<String> {
        conn.query_row(
            "SELECT first_call_id FROM sessions WHERE id = ?1",
            [session],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn add_call(conn: &Connection, id: &str, session: &str, ts: i64, sidechain: bool) {
        conn.execute(
            "INSERT INTO calls (message_id,session_id,ts,model,input_tokens,is_sidechain) VALUES (?1,?2,?3,'claude-sonnet-4-5',1,?4)",
            rusqlite::params![id, session, ts, sidechain],
        )
        .unwrap();
        crate::derived::refresh(conn).unwrap();
    }

    #[test]
    fn la_primera_llamada_de_la_sesion_es_la_de_menor_ts_y_orden() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        assert_eq!(first_call(&conn, "s1").as_deref(), Some("m1"));
        assert_eq!(first_call(&conn, "s3").as_deref(), Some("m4"));
        add_call(&conn, "tarde", "s1", 999_999, false);
        assert_eq!(first_call(&conn, "s1").as_deref(), Some("m1"));
        add_call(&conn, "antes", "s1", 500, false);
        assert_eq!(first_call(&conn, "s1").as_deref(), Some("antes"));
        add_call(&conn, "empate", "s1", 500, false);
        assert_eq!(
            first_call(&conn, "s1").as_deref(),
            Some("antes"),
            "con el mismo ts gana la que se escribió antes"
        );
        conn.execute("UPDATE calls SET ts = 100 WHERE message_id = 'tarde'", [])
            .unwrap();
        crate::derived::refresh(&conn).unwrap();
        assert_eq!(first_call(&conn, "s1").as_deref(), Some("tarde"));
        conn.execute("DELETE FROM calls WHERE message_id = 'tarde'", [])
            .unwrap();
        crate::derived::refresh(&conn).unwrap();
        assert_eq!(first_call(&conn, "s1").as_deref(), Some("antes"));
    }

    #[test]
    fn una_llamada_de_subagente_nunca_es_la_primera() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        add_call(&conn, "sub", "s2", 1, true);
        assert_eq!(first_call(&conn, "s2").as_deref(), Some("m3"));
        conn.execute_batch(
            "INSERT INTO sessions (id,agent_id,started_at,ended_at) VALUES ('solo-sub','codex',0,0);",
        )
        .unwrap();
        add_call(&conn, "sub2", "solo-sub", 1, true);
        assert_eq!(first_call(&conn, "solo-sub"), None);
    }

    #[test]
    fn el_desglose_no_numera_todas_las_llamadas() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let log = db::trace::start(&conn);
        for by in ["agent", "project", "branch", "model", "activity"] {
            breakdown(&conn, &Filter::default(), by).unwrap();
        }
        let sql: Vec<String> = db::trace::take(log)
            .into_iter()
            .map(|e| e.sql)
            .filter(|s| s.contains("GROUP BY"))
            .collect();
        assert_eq!(sql.len(), 5);
        assert!(sql.iter().all(|s| !s.contains("ROW_NUMBER")), "{sql:?}");
    }

    // ---------------------------------------------------------------- ruta de agregados (D7)

    /// 2026-10-20T22:00:00Z: medianoche del 21 en Madrid (horario de verano).
    const MIDNIGHT: i64 = 1_792_533_600_000;

    fn madrid(conn: &Connection) {
        db::set_cache_tz(conn, "Europe/Madrid").unwrap();
    }

    fn range(from: Option<i64>, to: Option<i64>) -> Filter {
        Filter {
            from,
            to,
            ..Default::default()
        }
    }

    #[test]
    fn la_ruta_de_agregados_exige_dias_completos_de_la_zona_de_la_cache() {
        let conn = db::open_in_memory().unwrap();
        madrid(&conn);
        let days = |from, to| Source::Days { from, to };
        for (f, tz, expected) in [
            (range(None, None), None, days(None, None)),
            (
                range(Some(MIDNIGHT), None),
                None,
                days(Some(20261021), None),
            ),
            (
                range(Some(MIDNIGHT), Some(MIDNIGHT + 2 * DAY_MS)),
                Some("Europe/Madrid"),
                days(Some(20261021), Some(20261023)),
            ),
            (
                range(Some(MIDNIGHT), Some(MIDNIGHT + 7 * DAY_MS)),
                None,
                Source::Detail,
            ),
            // Una semana que cruza el cambio de hora: la medianoche del 28 es a las 23:00Z.
            (
                range(Some(MIDNIGHT), Some(MIDNIGHT + 7 * DAY_MS + HOUR_MS)),
                None,
                days(Some(20261021), Some(20261028)),
            ),
            (range(Some(MIDNIGHT + 1), None), None, Source::Detail),
            (range(Some(MIDNIGHT - HOUR_MS), None), None, Source::Detail),
            (range(None, Some(MIDNIGHT + HOUR_MS)), None, Source::Detail),
            (range(Some(MIDNIGHT), None), Some("UTC"), Source::Detail),
            (range(None, None), Some("America/New_York"), Source::Detail),
        ] {
            assert_eq!(source(&conn, &f, tz).unwrap(), expected, "{f:?} {tz:?}");
        }
    }

    fn sql_of(conn: &Connection, run: impl FnOnce()) -> Vec<String> {
        let log = db::trace::start(conn);
        run();
        db::trace::take(log).into_iter().map(|e| e.sql).collect()
    }

    #[test]
    fn con_dias_completos_las_consultas_no_leen_las_llamadas() {
        let conn = db::open_in_memory().unwrap();
        madrid(&conn);
        testdata::seed(&conn);
        let f = range(None, Some(MIDNIGHT));
        let reads_calls = |sql: &[String]| {
            sql.iter()
                .filter(|s| s.contains("call_costs") || s.contains("tool_calls t"))
                .count()
        };
        let sql = sql_of(&conn, || {
            for by in ["agent", "project", "branch", "model", "tool", "mcp"] {
                breakdown(&conn, &f, by).unwrap();
            }
            for by in ["agent", "model", "project", "branch", "tool"] {
                timeseries_by(&conn, &f, by, "Europe/Madrid").unwrap();
            }
            timeseries(&conn, &f, "day", "Europe/Madrid").unwrap();
            list_agents(&conn, &f).unwrap();
            list_projects(&conn, &f).unwrap();
        });
        assert_eq!(reads_calls(&sql), 0, "{sql:#?}");
        let sql = sql_of(&conn, || {
            summary(&conn, &f, 0).unwrap();
        });
        assert_eq!(reads_calls(&sql), 1, "solo el burn rate: {sql:#?}");
        let sql = sql_of(&conn, || {
            breakdown(&conn, &range(Some(1), Some(MIDNIGHT)), "model").unwrap();
        });
        assert_eq!(reads_calls(&sql), 1, "a mitad de día, detalle: {sql:#?}");
    }

    /// Todas las consultas con ruta de agregados, por la ruta elegida y forzando el detalle.
    fn both_routes(
        conn: &Connection,
        f: &Filter,
    ) -> Vec<(String, serde_json::Value, serde_json::Value)> {
        let tz = "Europe/Madrid";
        let days = source(conn, f, Some(tz)).unwrap();
        let json = |v: anyhow::Result<serde_json::Value>| v.unwrap();
        let mut out = vec![(
            "summary".to_string(),
            json(summary_from(conn, f, MIDNIGHT, days).map(|v| serde_json::to_value(v).unwrap())),
            json(
                summary_from(conn, f, MIDNIGHT, Source::Detail)
                    .map(|v| serde_json::to_value(v).unwrap()),
            ),
        )];
        out.push((
            "timeseries".into(),
            serde_json::to_value(timeseries_from(conn, f, "day", tz, days).unwrap()).unwrap(),
            serde_json::to_value(timeseries_from(conn, f, "day", tz, Source::Detail).unwrap())
                .unwrap(),
        ));
        for by in ["agent", "model", "project", "branch", "tool"] {
            out.push((
                format!("timeseries_by({by})"),
                serde_json::to_value(timeseries_by_from(conn, f, by, tz, days).unwrap()).unwrap(),
                serde_json::to_value(timeseries_by_from(conn, f, by, tz, Source::Detail).unwrap())
                    .unwrap(),
            ));
        }
        for by in ["agent", "project", "branch", "model", "tool", "mcp"] {
            out.push((
                format!("breakdown({by})"),
                serde_json::to_value(breakdown_from(conn, f, by, days).unwrap()).unwrap(),
                serde_json::to_value(breakdown_from(conn, f, by, Source::Detail).unwrap()).unwrap(),
            ));
        }
        out.push((
            "list_agents".into(),
            serde_json::to_value(list_agents_from(conn, f, days).unwrap()).unwrap(),
            serde_json::to_value(list_agents_from(conn, f, Source::Detail).unwrap()).unwrap(),
        ));
        out.push((
            "list_projects".into(),
            serde_json::to_value(list_projects_from(conn, f, days).unwrap()).unwrap(),
            serde_json::to_value(list_projects_from(conn, f, Source::Detail).unwrap()).unwrap(),
        ));
        out
    }

    fn assert_close(a: &serde_json::Value, b: &serde_json::Value, path: &str) {
        use serde_json::Value;
        match (a, b) {
            (Value::Array(x), Value::Array(y)) => {
                assert_eq!(x.len(), y.len(), "{path}: {a} / {b}");
                for (i, (x, y)) in x.iter().zip(y).enumerate() {
                    assert_close(x, y, &format!("{path}[{i}]"));
                }
            }
            (Value::Object(x), Value::Object(y)) => {
                assert_eq!(x.len(), y.len(), "{path}");
                for (k, v) in x {
                    assert_close(v, &y[k], &format!("{path}.{k}"));
                }
            }
            (Value::Number(x), Value::Number(y)) if x.is_f64() || y.is_f64() => {
                let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
                assert!(
                    (x - y).abs() <= 1e-9 * x.abs().max(1.0),
                    "{path}: {x} / {y}"
                );
            }
            _ => assert_eq!(a, b, "{path}"),
        }
    }

    #[test]
    fn las_dos_rutas_dan_lo_mismo_en_cada_particion_del_filtro() {
        let conn = db::open_in_memory().unwrap();
        madrid(&conn);
        testdata::seed(&conn);
        // Llamadas y herramientas en varios días alrededor del cambio de hora del 25 de octubre.
        for i in 0..40i64 {
            let ts = MIDNIGHT + i * 7 * HOUR_MS + 1234;
            let (s, model) = [
                ("s1", "claude-sonnet-4-5"),
                ("s2", "claude-haiku-4-5"),
                ("s3", "modelo-x"),
            ][(i % 3) as usize];
            conn.execute(
                "INSERT INTO calls (message_id,session_id,ts,model,input_tokens,output_tokens,cache_read,is_sidechain)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                rusqlite::params![format!("r{i}"), s, ts, model, 1000 + i, 10 * i, 500 * i, i % 7 == 0],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO tool_calls (call_id,session_id,ts,tool,is_error) VALUES (?1,?2,?3,?4,?5)",
                rusqlite::params![format!("u{i}"), s, ts, ["Bash", "Read", "mcp__srv__x"][(i % 3) as usize], i % 4 == 0],
            )
            .unwrap();
        }
        conn.execute_batch(
            "INSERT INTO sessions (id,agent_id,project_id,started_at,ended_at,is_subagent) VALUES ('sub','codex',NULL,0,0,1);
             INSERT INTO calls (message_id,session_id,ts,model,input_tokens) VALUES ('x1','sub',1792540800000,'claude-sonnet-4-5',9);",
        )
        .unwrap();
        crate::derived::refresh(&conn).unwrap();
        let week = MIDNIGHT + 7 * DAY_MS + HOUR_MS;
        for (name, f) in [
            ("todo", Filter::default()),
            ("dias completos", range(Some(MIDNIGHT + DAY_MS), Some(week))),
            (
                "mitad de dia",
                range(Some(MIDNIGHT + 13 * HOUR_MS), Some(week)),
            ),
            (
                "to en el futuro",
                range(Some(MIDNIGHT), Some(week + 400 * DAY_MS)),
            ),
            ("vacio", range(Some(week + 400 * DAY_MS), None)),
            (
                "agente y proyecto",
                Filter {
                    agents: Some(vec!["claude-code".into()]),
                    projects: Some(vec![1]),
                    ..range(Some(MIDNIGHT), None)
                },
            ),
            (
                "sin proyecto",
                Filter {
                    no_project: true,
                    ..Default::default()
                },
            ),
        ] {
            for (query, by_route, detail) in both_routes(&conn, &f) {
                assert_close(&by_route, &detail, &format!("{name} {query}"));
            }
        }
    }

    #[test]
    fn info_de_datos() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let info = data_info(&conn).unwrap();
        assert_eq!(
            (
                info.first_ts,
                info.calls,
                info.watched_files,
                info.last_scan
            ),
            (Some(1000), 4, 0, None)
        );
    }
}

/// Una fila por llamada, con proyecto/agente resueltos, para exportar.
#[tracing::instrument(level = "debug", skip_all, fields(format = %format))]
pub fn export(conn: &Connection, f: &Filter, format: &str) -> Result<String> {
    let (w, args) = f.sql("c.ts");
    let sql = format!(
        "SELECT c.ts, COALESCE(a.name, s.agent_id) AS agent, COALESCE(p.name, '') AS project,
                COALESCE(s.git_branch, '') AS branch, c.model, c.input_tokens, c.output_tokens,
                c.cache_read, c.cache_write, c.reasoning_tokens, c.cost_usd, c.is_sidechain
         FROM call_costs c JOIN sessions s ON s.id = c.session_id
         LEFT JOIN agents a ON a.id = s.agent_id
         LEFT JOIN projects p ON p.id = s.project_id
         WHERE {w} ORDER BY c.ts"
    );
    let mut stmt = conn.prepare(&sql)?;
    let cols = [
        "ts",
        "iso",
        "agent",
        "project",
        "branch",
        "model",
        "input_tokens",
        "output_tokens",
        "cache_read",
        "cache_write",
        "reasoning_tokens",
        "cost_usd",
        "is_subagent",
    ];
    let rows = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            let ts: i64 = r.get(0)?;
            let iso = chrono::DateTime::from_timestamp_millis(ts).map(|d| d.to_rfc3339()).unwrap_or_default();
            Ok(serde_json::json!({
                "ts": ts, "iso": iso,
                "agent": r.get::<_, String>(1)?, "project": r.get::<_, String>(2)?, "branch": r.get::<_, String>(3)?,
                "model": r.get::<_, String>(4)?,
                "input_tokens": r.get::<_, i64>(5)?, "output_tokens": r.get::<_, i64>(6)?,
                "cache_read": r.get::<_, i64>(7)?, "cache_write": r.get::<_, i64>(8)?, "reasoning_tokens": r.get::<_, i64>(9)?,
                "cost_usd": r.get::<_, f64>(10)?, "is_subagent": r.get::<_, bool>(11)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if format == "json" {
        return Ok(serde_json::to_string_pretty(&rows)?);
    }
    // CSV: cabecera + una fila por llamada; los textos se entrecomillan si hace falta.
    let mut out = String::from("");
    out.push_str(&cols.join(","));
    out.push('\n');
    for row in &rows {
        let cells: Vec<String> = cols
            .iter()
            .map(|c| match &row[*c] {
                serde_json::Value::Null => String::new(),
                serde_json::Value::String(s) => csv_cell(s),
                v => v.to_string(),
            })
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    Ok(out)
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod export_tests {
    use super::*;
    use crate::db;

    #[test]
    fn exporta_csv_y_json() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let csv = export(&conn, &Filter::default(), "csv").unwrap();
        assert!(csv
            .lines()
            .next()
            .unwrap()
            .starts_with("ts,iso,agent,project,branch,model"));
        assert_eq!(csv.lines().count(), 1 + 4, "cabecera + 4 llamadas");
        assert!(csv.contains("claude-sonnet-4-5"));
        let json = export(&conn, &Filter::default(), "json").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 4);
    }
}

/// Inicio del mes local en curso (epoch ms) y factor de proyección lineal a fin de mes
/// (`días del mes / días transcurridos`).
pub fn month_factor() -> (i64, f64) {
    use chrono::{Datelike, Local, TimeZone};
    let now = Local::now();
    let start = Local
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .map(|d| d.timestamp_millis())
        .unwrap_or(0);
    let (y, m) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    let next = Local
        .with_ymd_and_hms(y, m, 1, 0, 0, 0)
        .single()
        .map(|d| d.timestamp_millis())
        .unwrap_or(start + 30 * DAY_MS);
    let days_in_month = (next - start) as f64 / DAY_MS as f64;
    let day = now.day() as f64;
    (start, if day > 0.0 { days_in_month / day } else { 1.0 })
}

/// Gasto acumulado del mes en curso y proyección lineal a fin de mes (hora local del sistema).
pub fn month_progress(conn: &Connection, f: &Filter) -> Result<(f64, f64)> {
    let (start, factor) = month_factor();
    let month = Filter {
        from: Some(start),
        to: None,
        ..f.clone()
    };
    let (w, args) = month.sql("c.ts");
    let spent: f64 = conn.query_row(
        &format!("SELECT COALESCE(SUM(c.cost_usd),0) FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    Ok((spent, spent * factor))
}

#[cfg(test)]
mod span_tests {
    use super::*;
    use crate::providers::{claude_code::ClaudeCode, Provider};
    use crate::{db, ingest, insights, sessions};

    #[test]
    fn cada_consulta_de_los_comandos_abre_su_span() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/claude_code");
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(ClaudeCode::with_roots(vec![root]))];
        let mut conn = db::open_in_memory().unwrap();
        ingest::scan_all(&mut conn, &providers).unwrap();
        let f = Filter::default();
        let spans = crate::telemetry::tests::closed_spans(|| {
            summary(&conn, &f, 0).unwrap();
            breakdown(&conn, &f, "model").unwrap();
            timeseries(&conn, &f, "day", "UTC").unwrap();
            timeseries_by(&conn, &f, "agent", "UTC").unwrap();
            insights::activity(&conn, &f).unwrap();
            insights::activity_daily(&conn, &f, "UTC").unwrap();
            sessions::list_sessions(&conn, &f, None).unwrap();
        });
        let names: Vec<&str> = spans.iter().map(|s| s.name).collect();
        for expected in [
            "summary",
            "breakdown",
            "timeseries",
            "timeseries_by",
            "activity",
            "activity_daily",
            "list_sessions",
        ] {
            assert!(names.contains(&expected), "falta {expected}: {names:?}");
        }
        let by = spans.iter().find(|s| s.name == "breakdown").unwrap();
        assert_eq!(by.fields.get("by").map(String::as_str), Some("model"));
        let ts = spans.iter().find(|s| s.name == "timeseries").unwrap();
        assert_eq!(ts.fields.get("bucket").map(String::as_str), Some("day"));
    }
}
