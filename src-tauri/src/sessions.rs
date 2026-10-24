//! Explorador de sesiones: listado con métricas por sesión y detalle de una sesión
//! (coste acumulado, compactaciones, turnos, actividades, modelos y latencia de herramientas).

use crate::insights::ActivityRow;
use crate::insights::{classify_turn, TurnStats};
use crate::queries::{self, BreakdownRow, Filter, Point};
use anyhow::{Context, Result};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Máximo de sesiones que devuelve el listado (las más recientes).
pub const MAX_SESSIONS: usize = 500;

#[derive(Debug, Serialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub project_id: Option<i64>,
    pub project: Option<String>,
    pub branch: Option<String>,
    /// Primera y última llamada dentro del filtro (epoch ms).
    pub started_at: i64,
    pub ended_at: i64,
    /// Modelo con más coste (y, a igualdad, más llamadas).
    pub model: Option<String>,
    pub cost_usd: f64,
    pub calls: i64,
    pub cache_hit: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// `false` si alguna llamada no tiene precio (su coste cuenta como 0).
    pub has_price: bool,
    pub turns: i64,
    pub compactions: i64,
    pub tool_calls: i64,
    pub tool_errors: i64,
    /// Llamadas hechas dentro de subagentes lanzados desde esta sesión.
    pub subagent_calls: i64,
    /// La sesión entera es un subagente (Codex, OpenCode).
    pub is_subagent: bool,
    /// Raíz del repositorio del proyecto (agrupa worktrees); `None` sin proyecto.
    pub project_key: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionList {
    pub sessions: Vec<SessionRow>,
    /// Sesiones que casan con el filtro, aunque se devuelvan menos.
    pub total: i64,
}

/// «AND col >= from AND col < to» con los límites del filtro incrustados (son i64).
pub(crate) fn time_bounds(f: &Filter, col: &str) -> String {
    let mut s = String::new();
    if let Some(from) = f.from {
        s.push_str(&format!(" AND {col} >= {from}"));
    }
    if let Some(to) = f.to {
        s.push_str(&format!(" AND {col} < {to}"));
    }
    s
}

fn ratio(num: i64, den: i64) -> f64 {
    if den > 0 {
        num as f64 / den as f64
    } else {
        0.0
    }
}

/// Métricas por sesión de las llamadas que casan con `where_sql`, más recientes primero.
/// `f` acota también turnos, herramientas, compactaciones, modelo y final de sesión al
/// periodo del filtro: todo lo que se presenta pertenece al mismo intervalo.
fn query_sessions(
    conn: &Connection,
    f: &Filter,
    where_sql: &str,
    args: &[Value],
    limit: usize,
) -> Result<Vec<SessionRow>> {
    let (t_turns, t_events, t_tools, t_calls) = (
        time_bounds(f, "t.ts"),
        time_bounds(f, "e.ts"),
        time_bounds(f, "t.ts"),
        time_bounds(f, "ts"),
    );
    // El final de sesión tampoco sale del periodo.
    let ended = match f.to {
        Some(to) => format!("MIN(MAX(sc.last, COALESCE(s.ended_at, 0)), {to})"),
        None => "MAX(sc.last, COALESCE(s.ended_at, 0))".to_string(),
    };
    let sql = format!(
        "WITH sc AS (
           SELECT c.session_id AS sid, SUM(c.cost_usd) AS cost, COUNT(*) AS calls,
                  SUM(c.cache_read) AS cr, SUM(c.input_tokens + c.cache_read + c.cache_write) AS tin,
                  SUM(c.output_tokens) AS tout, MIN(c.has_price) AS hp, MIN(c.ts) AS first, MAX(c.ts) AS last,
                  SUM(c.is_sidechain) AS side
           FROM call_costs c JOIN sessions s ON s.id = c.session_id
           WHERE {where_sql} GROUP BY 1),
         top_model AS (
           SELECT session_id, model FROM (
             SELECT session_id, model,
                    ROW_NUMBER() OVER (PARTITION BY session_id ORDER BY SUM(cost_usd) DESC, COUNT(*) DESC, model) AS rn
             FROM call_costs WHERE session_id IN (SELECT sid FROM sc){t_calls} GROUP BY session_id, model)
           WHERE rn = 1)
         SELECT s.id, s.agent_id, COALESCE(a.name, s.agent_id), p.id, p.name, s.git_branch,
                sc.first, {ended}, tm.model,
                sc.cost, sc.calls, sc.cr, sc.tin, sc.tout, sc.hp, sc.side, s.is_subagent,
                (SELECT COUNT(*) FROM turns t WHERE t.session_id = s.id{t_turns}),
                (SELECT COUNT(*) FROM events e WHERE e.session_id = s.id AND e.kind = 'compaction'{t_events}),
                (SELECT COUNT(*) FROM tool_calls t WHERE t.session_id = s.id{t_tools}),
                (SELECT COALESCE(SUM(t.is_error), 0) FROM tool_calls t WHERE t.session_id = s.id{t_tools}),
                p.repo_root
         FROM sc JOIN sessions s ON s.id = sc.sid
         LEFT JOIN agents a ON a.id = s.agent_id
         LEFT JOIN projects p ON p.id = s.project_id
         LEFT JOIN top_model tm ON tm.session_id = s.id
         ORDER BY sc.first DESC, s.id LIMIT {limit}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            let started_at: i64 = r.get(6)?;
            let last: i64 = r.get(7)?;
            Ok(SessionRow {
                id: r.get(0)?,
                agent_id: r.get(1)?,
                agent_name: r.get(2)?,
                project_id: r.get(3)?,
                project: r.get(4)?,
                branch: r.get(5)?,
                started_at,
                // `ended_at` de la sesión puede quedar fuera del filtro: nunca antes del inicio.
                ended_at: last.max(started_at),
                model: r.get(8)?,
                cost_usd: r.get(9)?,
                calls: r.get(10)?,
                cache_hit: ratio(r.get(11)?, r.get(12)?),
                input_tokens: r.get(12)?,
                output_tokens: r.get(13)?,
                has_price: r.get(14)?,
                subagent_calls: r.get(15)?,
                is_subagent: r.get(16)?,
                turns: r.get(17)?,
                compactions: r.get(18)?,
                tool_calls: r.get(19)?,
                tool_errors: r.get(20)?,
                project_key: r.get(21)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Sesiones con al menos una llamada en el filtro; métricas calculadas con esas llamadas.
#[tracing::instrument(level = "debug", skip_all)]
pub fn list_sessions(conn: &Connection, f: &Filter, limit: Option<usize>) -> Result<SessionList> {
    let (w, args) = f.sql("c.ts");
    let limit = limit.unwrap_or(MAX_SESSIONS).min(MAX_SESSIONS);
    let sessions = query_sessions(conn, f, &w, &args, limit)?;
    let total = conn.query_row(
        &format!(
            "SELECT COUNT(DISTINCT c.session_id) FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w}"
        ),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    Ok(SessionList { sessions, total })
}

// ---------------------------------------------------------------------------
// Detalle

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CostPoint {
    pub ts: i64,
    /// Coste acumulado de la sesión hasta esta llamada (incluida).
    pub cost_usd: f64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnRow {
    /// Posición del turno en la sesión, desde 1.
    pub n: i64,
    pub id: String,
    pub ts: i64,
    pub activity: String,
    pub cost_usd: f64,
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// Herramientas del turno con su número de usos, más usadas primero.
    pub tools: Vec<(String, i64)>,
    pub tool_errors: i64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolStat {
    pub tool: String,
    pub calls: i64,
    pub errors: i64,
    pub p50_ms: Option<i64>,
    pub p95_ms: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyCost {
    pub key: String,
    pub cost_usd: f64,
    pub calls: i64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session: SessionRow,
    pub timeline: Vec<CostPoint>,
    pub compactions: Vec<i64>,
    pub turns: Vec<TurnRow>,
    pub activities: Vec<KeyCost>,
    pub models: Vec<KeyCost>,
    pub tools: Vec<ToolStat>,
}

/// Percentil `p` (0–100) por el método del rango más cercano; `None` si no hay valores.
pub fn percentile(sorted: &[i64], p: f64) -> Option<i64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    Some(sorted[rank.clamp(1, sorted.len()) - 1])
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn session_detail(conn: &Connection, id: &str) -> Result<SessionDetail> {
    // El detalle de una sesión muestra su historia completa: sin acotar por periodo.
    let session = query_sessions(
        conn,
        &Filter::default(),
        "c.session_id = ?",
        &[Value::from(id.to_string())],
        1,
    )?
    .into_iter()
    .next()
    .with_context(|| format!("no existe la sesión {id}"))?;

    // Coste acumulado llamada a llamada.
    let mut acc = 0.0;
    let mut stmt = conn.prepare(
        "SELECT ts, cost_usd FROM call_costs WHERE session_id = ?1 ORDER BY ts, message_id",
    )?;
    let timeline = stmt
        .query_map(params![id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
        })?
        .map(|x| {
            x.map(|(ts, c)| {
                acc += c;
                CostPoint { ts, cost_usd: acc }
            })
        })
        .collect::<rusqlite::Result<_>>()?;

    let mut stmt = conn.prepare(
        "SELECT ts FROM events WHERE session_id = ?1 AND kind = 'compaction' ORDER BY ts",
    )?;
    let compactions = stmt
        .query_map(params![id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;

    // Turnos: mismas estadísticas que By Activity, pero solo de esta sesión.
    let mut stats: BTreeMap<(i64, String), TurnStats> = BTreeMap::new();
    let mut key_of: HashMap<String, (i64, String)> = HashMap::new();
    let mut stmt = conn.prepare("SELECT id, ts, intent FROM turns WHERE session_id = ?1")?;
    let mut rows = stmt.query(params![id])?;
    while let Some(r) = rows.next()? {
        let tid: String = r.get(0)?;
        let key = (r.get::<_, i64>(1)?, tid.clone());
        let t = stats.entry(key.clone()).or_default();
        t.ts = key.0;
        t.intent = r.get(2)?;
        key_of.insert(tid, key);
    }
    let mut tokens: HashMap<String, (i64, i64, i64)> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT turn_id, model, SUM(cost_usd), COUNT(*), SUM(input_tokens + cache_read + cache_write), SUM(output_tokens)
         FROM call_costs WHERE session_id = ?1 AND turn_id IS NOT NULL GROUP BY 1, 2",
    )?;
    let mut rows = stmt.query(params![id])?;
    while let Some(r) = rows.next()? {
        let tid: String = r.get(0)?;
        let Some(key) = key_of.get(&tid) else {
            continue;
        };
        let t = stats.get_mut(key).expect("turno indexado");
        t.cost_usd += r.get::<_, f64>(2)?;
        let n: i64 = r.get(3)?;
        *t.models.entry(r.get(1)?).or_default() += n;
        let e = tokens.entry(tid).or_default();
        e.0 += n;
        e.1 += r.get::<_, i64>(4)?;
        e.2 += r.get::<_, i64>(5)?;
    }
    let mut stmt = conn.prepare(
        "SELECT turn_id, tool, target, is_error FROM tool_calls WHERE session_id = ?1 AND turn_id IS NOT NULL",
    )?;
    let mut rows = stmt.query(params![id])?;
    while let Some(r) = rows.next()? {
        let tid: String = r.get(0)?;
        if let Some(key) = key_of.get(&tid) {
            let t = stats.get_mut(key).expect("turno indexado");
            t.tools.push((r.get(1)?, r.get(2)?, r.get(3)?));
        }
    }

    let mut by_activity: HashMap<String, (f64, i64)> = HashMap::new();
    let turns: Vec<TurnRow> = stats
        .iter()
        .enumerate()
        .map(|(i, ((ts, tid), t))| {
            let activity = classify_turn(t).to_string();
            let (calls, input, output) = tokens.get(tid).copied().unwrap_or_default();
            let a = by_activity.entry(activity.clone()).or_default();
            a.0 += t.cost_usd;
            a.1 += calls;
            let mut counts: HashMap<&str, i64> = HashMap::new();
            for (tool, _, _) in &t.tools {
                *counts.entry(tool.as_str()).or_default() += 1;
            }
            let mut tools: Vec<(String, i64)> = counts
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();
            tools.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            TurnRow {
                n: i as i64 + 1,
                id: tid.clone(),
                ts: *ts,
                activity,
                cost_usd: t.cost_usd,
                calls,
                input_tokens: input,
                output_tokens: output,
                tools,
                tool_errors: t.tools.iter().filter(|x| x.2).count() as i64,
            }
        })
        .collect();

    // Llamadas sin turno (agentes que no registran prompts): por su actividad de llamada.
    let mut stmt = conn.prepare(
        "SELECT COALESCE(activity, 'conversation'), SUM(cost_usd), COUNT(*) FROM call_costs
         WHERE session_id = ?1 AND (turn_id IS NULL OR turn_id NOT IN (SELECT id FROM turns WHERE session_id = ?1))
         GROUP BY 1",
    )?;
    let mut rows = stmt.query(params![id])?;
    while let Some(r) = rows.next()? {
        let a = by_activity.entry(r.get(0)?).or_default();
        a.0 += r.get::<_, f64>(1)?;
        a.1 += r.get::<_, i64>(2)?;
    }
    let mut activities: Vec<KeyCost> = by_activity
        .into_iter()
        .map(|(key, (cost_usd, calls))| KeyCost {
            key,
            cost_usd,
            calls,
        })
        .collect();
    activities.sort_by(|a, b| b.cost_usd.total_cmp(&a.cost_usd).then(a.key.cmp(&b.key)));

    let mut stmt = conn.prepare(
        "SELECT model, SUM(cost_usd), COUNT(*) FROM call_costs WHERE session_id = ?1
         GROUP BY 1 ORDER BY 2 DESC, 3 DESC, 1",
    )?;
    let models = stmt
        .query_map(params![id], |r| {
            Ok(KeyCost {
                key: r.get(0)?,
                cost_usd: r.get(1)?,
                calls: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    // Herramientas con latencia: percentiles sobre los usos que tienen duración.
    let mut per_tool: BTreeMap<String, (i64, i64, Vec<i64>)> = BTreeMap::new();
    let mut stmt =
        conn.prepare("SELECT tool, is_error, duration_ms FROM tool_calls WHERE session_id = ?1")?;
    let mut rows = stmt.query(params![id])?;
    while let Some(r) = rows.next()? {
        let e = per_tool.entry(r.get(0)?).or_default();
        e.0 += 1;
        e.1 += r.get::<_, bool>(1)? as i64;
        if let Some(d) = r.get::<_, Option<i64>>(2)? {
            e.2.push(d);
        }
    }
    let mut tools: Vec<ToolStat> = per_tool
        .into_iter()
        .map(|(tool, (calls, errors, mut d))| {
            d.sort_unstable();
            ToolStat {
                tool,
                calls,
                errors,
                p50_ms: percentile(&d, 50.0),
                p95_ms: percentile(&d, 95.0),
            }
        })
        .collect();
    tools.sort_by(|a, b| b.calls.cmp(&a.calls).then(a.tool.cmp(&b.tool)));

    Ok(SessionDetail {
        session,
        timeline,
        compactions,
        turns,
        activities,
        models,
        tools,
    })
}

// ---------------------------------------------------------------------------
// Proyectos: las mismas métricas, agregadas por raíz de repositorio

/// Tope de sesiones que se agregan en el listado de proyectos.
const MAX_AGGREGATED: usize = 100_000;

#[derive(Debug, Serialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    /// Raíz del repositorio; `""` para las sesiones sin proyecto.
    pub key: String,
    /// Un id de proyecto de ese repo, para filtrar por él; `None` sin proyecto.
    pub project_id: Option<i64>,
    pub name: String,
    pub path: Option<String>,
    pub agents: Vec<AgentRef>,
    pub branches: Vec<String>,
    /// Modelo con más coste en el proyecto.
    pub model: Option<String>,
    /// Sesiones principales (sin contar las que son subagentes).
    pub sessions: i64,
    /// Suma de la duración de sus sesiones (ms).
    pub active_ms: i64,
    pub turns: i64,
    pub compactions: i64,
    pub tool_calls: i64,
    pub tool_errors: i64,
    pub subagent_calls: i64,
    pub cost_usd: f64,
    pub calls: i64,
    pub cache_hit: f64,
    pub has_price: bool,
    pub first_ts: i64,
    pub last_ts: i64,
}

/// Agrega sesiones en proyectos (por `project_key`), del más caro al más barato. El nombre de
/// cada proyecto es el mismo que en By Project (`MIN(name)` de las carpetas del repo).
fn summarize(
    conn: &Connection,
    rows: &[SessionRow],
    top_model: &HashMap<String, String>,
) -> Result<Vec<ProjectSummary>> {
    /// Proyecto en construcción: sus cifras y lo que falta para cerrarlas.
    struct Acc {
        p: ProjectSummary,
        branches: BTreeSet<String>,
        /// id → nombre
        agents: BTreeMap<String, String>,
        cache_read: f64,
        input: i64,
    }
    let mut acc: BTreeMap<String, Acc> = BTreeMap::new();
    for r in rows {
        let key = r.project_key.clone().unwrap_or_default();
        let e = acc.entry(key.clone()).or_insert_with(|| Acc {
            p: ProjectSummary {
                key: key.clone(),
                project_id: r.project_id,
                name: r.project.clone().unwrap_or_else(|| "(sin proyecto)".into()),
                path: r.project_key.clone(),
                agents: Vec::new(),
                branches: Vec::new(),
                model: top_model.get(&key).cloned(),
                sessions: 0,
                active_ms: 0,
                turns: 0,
                compactions: 0,
                tool_calls: 0,
                tool_errors: 0,
                subagent_calls: 0,
                cost_usd: 0.0,
                calls: 0,
                cache_hit: 0.0,
                has_price: true,
                first_ts: r.started_at,
                last_ts: r.ended_at,
            },
            branches: BTreeSet::new(),
            agents: BTreeMap::new(),
            cache_read: 0.0,
            input: 0,
        });
        let Acc {
            p,
            branches,
            agents,
            cache_read,
            input,
        } = e;
        // Para filtrar basta un id cualquiera del repo: el filtro incluye todos sus worktrees.
        if r.project_id.is_some() && (p.project_id.is_none() || r.project_id < p.project_id) {
            p.project_id = r.project_id;
        }
        if r.is_subagent {
            p.subagent_calls += r.calls;
        } else {
            p.sessions += 1;
            p.subagent_calls += r.subagent_calls;
        }
        p.active_ms += (r.ended_at - r.started_at).max(0);
        p.turns += r.turns;
        p.compactions += r.compactions;
        p.tool_calls += r.tool_calls;
        p.tool_errors += r.tool_errors;
        p.cost_usd += r.cost_usd;
        p.calls += r.calls;
        p.has_price &= r.has_price;
        p.first_ts = p.first_ts.min(r.started_at);
        p.last_ts = p.last_ts.max(r.ended_at);
        *cache_read += r.cache_hit * r.input_tokens as f64;
        *input += r.input_tokens;
        if let Some(b) = &r.branch {
            branches.insert(b.clone());
        }
        agents.insert(r.agent_id.clone(), r.agent_name.clone());
    }
    let mut out: Vec<ProjectSummary> = acc
        .into_values()
        .map(
            |Acc {
                 mut p,
                 branches,
                 agents,
                 cache_read,
                 input,
             }| {
                p.branches = branches.into_iter().collect();
                p.agents = agents
                    .into_iter()
                    .map(|(id, name)| AgentRef { id, name })
                    .collect();
                p.cache_hit = if input > 0 {
                    cache_read / input as f64
                } else {
                    0.0
                };
                p
            },
        )
        .collect();
    for p in out.iter_mut().filter(|p| !p.key.is_empty()) {
        if let Some(name) = conn
            .query_row(
                "SELECT MIN(name) FROM projects WHERE repo_root = ?1",
                params![p.key],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten()
        {
            p.name = name;
        }
    }
    out.sort_by(|a, b| b.cost_usd.total_cmp(&a.cost_usd).then(a.name.cmp(&b.name)));
    Ok(out)
}

/// Modelo con más coste de cada proyecto (clave = raíz del repo, `""` sin proyecto).
fn top_models(conn: &Connection, f: &Filter) -> Result<HashMap<String, String>> {
    let (w, args) = f.sql("c.ts");
    let mut stmt = conn.prepare(&format!(
        "SELECT COALESCE(p.repo_root, ''), c.model, SUM(c.cost_usd), COUNT(*)
         FROM call_costs c JOIN sessions s ON s.id = c.session_id
         LEFT JOIN projects p ON p.id = s.project_id
         WHERE {w} GROUP BY 1, 2 ORDER BY 1, 3 DESC, 4 DESC, 2 DESC"
    ))?;
    let mut out = HashMap::new();
    let mut rows = stmt.query(params_from_iter(args.iter()))?;
    while let Some(r) = rows.next()? {
        out.entry(r.get(0)?).or_insert(r.get(1)?);
    }
    Ok(out)
}

/// Proyectos con al menos una llamada en el filtro, con métricas de esas llamadas.
#[tracing::instrument(name = "list_project_summaries", level = "debug", skip_all)]
pub fn list_projects(conn: &Connection, f: &Filter) -> Result<Vec<ProjectSummary>> {
    let (w, args) = f.sql("c.ts");
    let rows = query_sessions(conn, f, &w, &args, MAX_AGGREGATED)?;
    summarize(conn, &rows, &top_models(conn, f)?)
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetail {
    pub project: ProjectSummary,
    pub daily: Vec<Point>,
    pub activities: Vec<ActivityRow>,
    pub models: Vec<BreakdownRow>,
    pub branches: Vec<BreakdownRow>,
    pub sessions: Vec<SessionRow>,
    pub tools: Vec<ToolStat>,
}

/// Detalle de un proyecto (por su clave de `list_projects`) dentro del filtro.
#[tracing::instrument(level = "debug", skip_all)]
pub fn project_detail(conn: &Connection, key: &str, f: &Filter, tz: &str) -> Result<ProjectDetail> {
    let (w, args) = f.sql("c.ts");
    let (w, mut args) = if key.is_empty() {
        (format!("{w} AND s.project_id IS NULL"), args)
    } else {
        (
            format!("{w} AND s.project_id IN (SELECT id FROM projects WHERE repo_root = ?)"),
            args,
        )
    };
    if !key.is_empty() {
        args.push(Value::from(key.to_string()));
    }
    let sessions = query_sessions(conn, f, &w, &args, MAX_AGGREGATED)?;
    let project = summarize(conn, &sessions, &top_models(conn, f)?)?
        .into_iter()
        .find(|p| p.key == key)
        .with_context(|| format!("no hay actividad del proyecto {key} en el periodo"))?;

    // Las series y desgloses usan el filtro común acotado a ese proyecto (incluye sus
    // worktrees); el cubo «(sin proyecto)» se expresa con `no_project`.
    let pf = match project.project_id {
        Some(id) => Filter {
            projects: Some(vec![id]),
            ..f.clone()
        },
        None => Filter {
            no_project: true,
            ..f.clone()
        },
    };
    let (daily, activities, models, branches) = (
        queries::timeseries(conn, &pf, "day", tz)?,
        crate::insights::activity(conn, &pf)?.activities,
        queries::breakdown(conn, &pf, "model")?,
        queries::breakdown(conn, &pf, "branch")?,
    );

    // Latencia de herramientas de todas sus sesiones.
    let ids: Vec<Value> = sessions.iter().map(|s| Value::from(s.id.clone())).collect();
    let mut per_tool: BTreeMap<String, (i64, i64, Vec<i64>)> = BTreeMap::new();
    for chunk in ids.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(",");
        let bounds = time_bounds(f, "ts");
        let mut stmt = conn.prepare(&format!(
            "SELECT tool, is_error, duration_ms FROM tool_calls WHERE session_id IN ({marks}){bounds}"
        ))?;
        let mut rows = stmt.query(params_from_iter(chunk.iter()))?;
        while let Some(r) = rows.next()? {
            let e = per_tool.entry(r.get(0)?).or_default();
            e.0 += 1;
            e.1 += r.get::<_, bool>(1)? as i64;
            if let Some(d) = r.get::<_, Option<i64>>(2)? {
                e.2.push(d);
            }
        }
    }
    let mut tools: Vec<ToolStat> = per_tool
        .into_iter()
        .map(|(tool, (calls, errors, mut d))| {
            d.sort_unstable();
            ToolStat {
                tool,
                calls,
                errors,
                p50_ms: percentile(&d, 50.0),
                p95_ms: percentile(&d, 95.0),
            }
        })
        .collect();
    tools.sort_by(|a, b| b.calls.cmp(&a.calls).then(a.tool.cmp(&b.tool)));

    Ok(ProjectDetail {
        project,
        daily,
        activities,
        models,
        branches,
        sessions,
        tools,
    })
}

/// `true` si existe una sesión con ese id (para errores claros en el servidor MCP).
pub fn exists(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM sessions WHERE id = ?1", params![id], |_| {
            Ok(())
        })
        .optional()?
        .is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::queries::testdata;

    fn seed_detail(conn: &Connection) {
        testdata::seed(conn);
        conn.execute_batch(
            "INSERT INTO turns (id, session_id, ts, intent) VALUES ('t1','s1',1000,NULL),('t2','s1',4000,'debug');
             UPDATE calls SET turn_id = 't1' WHERE message_id = 'm1';
             UPDATE calls SET turn_id = 't2' WHERE message_id = 'm2';
             INSERT INTO calls (message_id,session_id,ts,model,input_tokens,output_tokens,turn_id)
               VALUES ('m5','s1',4500,'claude-haiku-4-5',1000000,0,'t2');
             INSERT INTO tool_calls (call_id,message_id,session_id,ts,tool,target,is_error,duration_ms,turn_id) VALUES
               ('u1','m1','s1',1000,'Read','/a',0,100,'t1'),
               ('u2','m2','s1',4000,'Edit','/a',0,200,'t2'),
               ('u3','m2','s1',4001,'Bash','cargo test',1,3000,'t2'),
               ('u4','m2','s1',4002,'Bash','cargo test',0,NULL,'t2');
             INSERT INTO events (session_id, ts, kind) VALUES ('s1', 3000, 'compaction');",
        )
        .unwrap();
        crate::derived::refresh(conn).unwrap();
    }

    #[test]
    fn listado_con_metricas_por_sesion() {
        let conn = db::open_in_memory().unwrap();
        seed_detail(&conn);
        let l = list_sessions(&conn, &Filter::default(), None).unwrap();
        assert_eq!(l.total, 3);
        // Más reciente primero.
        assert_eq!(
            l.sessions.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec!["s3", "s2", "s1"]
        );
        let s1 = &l.sessions[2];
        // 3 (m1) + 1.5 (m2) + 1 (m5 haiku)
        assert!((s1.cost_usd - 5.5).abs() < 1e-9);
        assert_eq!(s1.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(
            (s1.turns, s1.compactions, s1.tool_calls, s1.tool_errors),
            (2, 1, 4, 1)
        );
        assert_eq!(s1.project.as_deref(), Some("web"));
        assert!(!l.sessions[0].has_price); // s3 usa un modelo sin precio
    }

    #[test]
    fn detalle_de_sin_proyecto_con_series_y_desgloses() {
        let conn = db::open_in_memory().unwrap();
        seed_detail(&conn);
        conn.execute_batch(
            "INSERT INTO sessions (id, agent_id, started_at, ended_at) VALUES ('sn','claude-code',1000,2000);
             INSERT INTO calls (message_id,session_id,ts,model,input_tokens,output_tokens)
               VALUES ('mn','sn',1500,'claude-sonnet-4-5',1000000,0);",
        )
        .unwrap();
        crate::derived::refresh(&conn).unwrap();
        let d = project_detail(&conn, "", &Filter::default(), "UTC").unwrap();
        assert!(d.project.cost_usd > 0.0);
        assert!(!d.daily.is_empty(), "la serie diaria no puede venir vacía");
        assert!(!d.models.is_empty(), "el desglose de modelos tampoco");
        // Y solo con lo suyo: nada de los proyectos web/api.
        assert!((d.project.cost_usd - 3.0).abs() < 1e-9);
        assert!((d.daily.iter().map(|p| p.cost_usd).sum::<f64>() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn listado_respeta_el_periodo() {
        let conn = db::open_in_memory().unwrap();
        seed_detail(&conn);
        let f = Filter {
            from: Some(3500),
            to: Some(6000),
            ..Default::default()
        };
        let l = list_sessions(&conn, &f, None).unwrap();
        assert_eq!(l.total, 1);
        // Solo m2 (1.5) y m5 (1) caen en el periodo.
        assert!((l.sessions[0].cost_usd - 2.5).abs() < 1e-9);
        assert_eq!(l.sessions[0].started_at, 4000);
        // Las demás métricas también son del periodo, no de toda la historia de la sesión:
        // queda fuera el turno t1 (ts 1000), la herramienta u1 y la compactación (ts 3000).
        let s1 = &l.sessions[0];
        assert_eq!(
            (s1.turns, s1.compactions, s1.tool_calls, s1.tool_errors),
            (1, 0, 3, 1)
        );
        assert!(s1.ended_at <= 6000, "el final no sale del periodo");
    }

    #[test]
    fn detalle_de_sesion() {
        let conn = db::open_in_memory().unwrap();
        seed_detail(&conn);
        let d = session_detail(&conn, "s1").unwrap();
        assert_eq!(d.timeline.len(), 3);
        assert!((d.timeline.last().unwrap().cost_usd - 5.5).abs() < 1e-9);
        assert_eq!(d.compactions, vec![3000]);
        assert_eq!(d.turns.len(), 2);
        assert_eq!(d.turns[0].n, 1);
        assert_eq!(d.turns[1].tools[0], ("Bash".to_string(), 2));
        assert_eq!(d.turns[1].tool_errors, 1);
        assert_eq!((d.turns[1].calls, d.turns[1].input_tokens), (2, 1_000_000));
        assert_eq!(d.turns[1].activity, "debugging");
        let bash = d.tools.iter().find(|t| t.tool == "Bash").unwrap();
        assert_eq!(
            (bash.calls, bash.errors, bash.p50_ms, bash.p95_ms),
            (2, 1, Some(3000), Some(3000))
        );
        assert_eq!(d.models[0].key, "claude-sonnet-4-5");
    }

    #[test]
    fn sesion_inexistente() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        assert!(session_detail(&conn, "nada").is_err());
        assert!(!exists(&conn, "nada").unwrap());
        assert!(exists(&conn, "s1").unwrap());
    }

    #[test]
    fn percentiles() {
        let v = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&v, 50.0), Some(50));
        assert_eq!(percentile(&v, 95.0), Some(100));
        assert_eq!(percentile(&[], 50.0), None);
        assert_eq!(percentile(&[7], 95.0), Some(7));
    }

    #[test]
    fn proyectos_agregan_sesiones_y_worktrees() {
        let conn = db::open_in_memory().unwrap();
        seed_detail(&conn);
        // Worktree del repo de «web» (/w) y una sesión sin proyecto.
        conn.execute_batch(
            "INSERT INTO projects (id,name,cwd,repo_root) VALUES (3,'web-feat','/w-feat','/w');
             INSERT INTO sessions (id,agent_id,project_id,git_branch,started_at,ended_at) VALUES ('s4','codex',3,'feat',6000,7000),('s5','codex',NULL,NULL,8000,8000);
             INSERT INTO calls (message_id,session_id,ts,model,input_tokens,output_tokens) VALUES ('m6','s4',6000,'claude-haiku-4-5',1000000,0),('m7','s5',8000,'claude-haiku-4-5',10,0);",
        )
        .unwrap();
        crate::derived::refresh(&conn).unwrap();
        let ps = list_projects(&conn, &Filter::default()).unwrap();
        let web = ps.iter().find(|p| p.key == "/w").unwrap();
        assert_eq!(web.name, "web", "nombre del repo, no del worktree");
        assert_eq!(web.sessions, 3, "s1, s3 y el worktree s4");
        // s1 5.5 + s3 0 (sin precio) + s4 1
        assert!((web.cost_usd - 6.5).abs() < 1e-9);
        assert_eq!(web.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(
            web.agents.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            vec!["claude-code", "codex"]
        );
        assert_eq!(web.branches, vec!["feat".to_string(), "main".to_string()]);
        assert!(!web.has_price, "s3 usa un modelo sin precio");
        assert_eq!(ps[0].key, "/w", "el más caro primero");
        assert!(ps
            .iter()
            .any(|p| p.key.is_empty() && p.name == "(sin proyecto)"));
    }

    #[test]
    fn detalle_de_proyecto() {
        let conn = db::open_in_memory().unwrap();
        seed_detail(&conn);
        let d = project_detail(&conn, "/w", &Filter::default(), "UTC").unwrap();
        assert_eq!(d.sessions.len(), 2);
        assert_eq!(d.models[0].key, "claude-sonnet-4-5");
        assert!(d.branches.iter().any(|b| b.key == "main"));
        assert!(!d.daily.is_empty());
        let bash = d.tools.iter().find(|t| t.tool == "Bash").unwrap();
        assert_eq!((bash.calls, bash.errors), (2, 1));
        assert!(project_detail(&conn, "/no-existe", &Filter::default(), "UTC").is_err());
    }
}
