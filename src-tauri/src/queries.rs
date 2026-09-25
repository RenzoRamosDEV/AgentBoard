//! Consultas agregadas que alimentan la UI. Casi todo es un GROUP BY sobre `call_costs`.

use anyhow::Result;
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};
use serde::{Deserialize, Serialize};

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
}

impl Filter {
    /// `WHERE …` sobre `c` (call_costs o tabla con `ts`/`session_id`) unida a `s` (sessions).
    pub fn sql(&self, ts_col: &str) -> (String, Vec<Value>) {
        let mut conds = vec!["1=1".to_string()];
        let mut args: Vec<Value> = Vec::new();
        if let Some(from) = self.from {
            conds.push(format!("{ts_col} >= ?"));
            args.push(from.into());
        }
        if let Some(to) = self.to {
            conds.push(format!("{ts_col} < ?"));
            args.push(to.into());
        }
        if let Some(agents) = &self.agents {
            conds.push(format!("s.agent_id IN ({})", placeholders(agents.len())));
            args.extend(agents.iter().cloned().map(Value::from));
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

pub fn summary(conn: &Connection, f: &Filter, now: i64) -> Result<Summary> {
    let (w, args) = f.sql("c.ts");
    let sql = format!(
        "SELECT COALESCE(SUM(c.cost_usd),0), COUNT(*), COUNT(DISTINCT CASE WHEN s.is_subagent = 0 THEN c.session_id END),
                COALESCE(SUM(c.input_tokens),0), COALESCE(SUM(c.output_tokens),0),
                COALESCE(SUM(c.cache_read),0), COALESCE(SUM(c.cache_write),0),
                COALESCE(SUM(c.cache_savings_usd),0), MIN(c.ts), MAX(c.ts)
         FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w}"
    );
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
    out.cache_hit = ratio(out.cache_read, out.input_tokens + out.cache_read + out.cache_write);

    let sql = format!(
        "SELECT DISTINCT c.model FROM call_costs c JOIN sessions s ON s.id = c.session_id
         WHERE {w} AND NOT c.has_price ORDER BY 1"
    );
    let mut stmt = conn.prepare(&sql)?;
    out.unpriced_models = stmt
        .query_map(params_from_iter(args.iter()), |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;

    let recent = Filter { from: Some(now - HOUR_MS), to: None, ..f.clone() };
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
}

/// Coste por día u hora local. `tz_offset_min` = minutos a sumar a UTC para la hora local.
pub fn timeseries(conn: &Connection, f: &Filter, bucket: &str, tz_offset_min: i64) -> Result<Vec<Point>> {
    let size = if bucket == "hour" { HOUR_MS } else { DAY_MS };
    let off = tz_offset_min * 60_000;
    let (w, mut args) = f.sql("c.ts");
    let sql = format!(
        "SELECT ((c.ts + ?) / ?) * ? - ? AS b, SUM(c.cost_usd), COUNT(*)
         FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w}
         GROUP BY b ORDER BY b"
    );
    let mut all: Vec<Value> = vec![off.into(), size.into(), size.into(), off.into()];
    all.append(&mut args);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(all.iter()), |r| Ok(Point { ts: r.get(0)?, cost_usd: r.get(1)?, calls: r.get(2)? }))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
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
}

impl BreakdownRow {
    pub fn simple(key: impl Into<String>, calls: i64, errors: i64, cost_usd: f64) -> Self {
        let key = key.into();
        Self { label: key.clone(), key, cost_usd, calls, errors, cache_hit: 0.0, has_price: true, sessions: 0, overhead_tokens: 0.0 }
    }
}

/// Coste y uso agrupado por `agent | project | branch | model | activity | tool | command | skill | mcp | agent_type`.
pub fn breakdown(conn: &Connection, f: &Filter, by: &str) -> Result<Vec<BreakdownRow>> {
    match by {
        "command" => return crate::insights::shell_commands(conn, f),
        "skill" => return crate::insights::skills_and_agents(conn, f),
        "mcp" => return crate::insights::mcp_servers(conn, f),
        "agent_type" => return crate::insights::agent_types(conn, f),
        _ => {}
    }
    let (w, args) = f.sql(if by == "tool" { "t.ts" } else { "c.ts" });
    // `firsts` = primera llamada principal de cada sesión, para el overhead de contexto.
    let calls = |key: &str, label: &str, join: &str| {
        format!(
            "WITH firsts AS (
               SELECT message_id FROM (
                 SELECT message_id, ROW_NUMBER() OVER (PARTITION BY session_id ORDER BY ts) AS rn
                 FROM calls WHERE is_sidechain = 0) WHERE rn = 1)
             SELECT {key}, {label}, SUM(c.cost_usd), COUNT(*), 0,
                    SUM(c.cache_read), SUM(c.input_tokens + c.cache_read + c.cache_write), MIN(c.has_price),
                    COUNT(DISTINCT CASE WHEN s.is_subagent = 0 THEN c.session_id END),
                    COALESCE(AVG(CASE WHEN f.message_id IS NOT NULL THEN c.input_tokens + c.cache_read + c.cache_write END), 0)
             FROM call_costs c JOIN sessions s ON s.id = c.session_id {join}
             LEFT JOIN firsts f ON f.message_id = c.message_id
             WHERE {w} GROUP BY 1 ORDER BY 3 DESC, 4 DESC"
        )
    };
    let tools = |key: &str, extra: &str| {
        format!(
            "SELECT {key}, {key}, 0.0, COUNT(*), SUM(t.is_error), 0, 0, 1, 0, 0.0
             FROM tool_calls t JOIN sessions s ON s.id = t.session_id
             WHERE {w} {extra} GROUP BY 1 ORDER BY 4 DESC LIMIT 50"
        )
    };
    let sql = match by {
        "agent" => calls("s.agent_id", "COALESCE(a.name, s.agent_id)", "LEFT JOIN agents a ON a.id = s.agent_id"),
        "project" => calls("COALESCE(p.repo_root, '')", "COALESCE(MIN(p.name), '(sin proyecto)')", "LEFT JOIN projects p ON p.id = s.project_id"),
        "branch" => calls("COALESCE(s.git_branch, '')", "COALESCE(s.git_branch, '(sin rama)')", ""),
        "model" => calls("c.model", "c.model", ""),
        "activity" => calls("COALESCE(c.activity, 'conversation')", "COALESCE(c.activity, 'conversation')", ""),
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
pub fn list_agents(conn: &Connection, f: &Filter) -> Result<Vec<AgentRow>> {
    let period = Filter { from: f.from, to: f.to, ..Default::default() };
    let (w, args) = period.sql("c.ts");
    let sql = format!(
        "SELECT a.id, a.name, a.log_root, COALESCE(x.cost, 0), COALESCE(x.n, 0)
         FROM agents a LEFT JOIN (
           SELECT s.agent_id AS id, SUM(c.cost_usd) AS cost, COUNT(*) AS n
           FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w} GROUP BY 1
         ) x ON x.id = a.id
         ORDER BY 4 DESC, 5 DESC, a.name"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(AgentRow { id: r.get(0)?, name: r.get(1)?, log_root: r.get(2)?, cost_usd: r.get(3)?, calls: r.get(4)? })
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
pub fn list_projects(conn: &Connection, f: &Filter) -> Result<Vec<ProjectRow>> {
    let scope = Filter { projects: None, ..f.clone() };
    let (w, args) = scope.sql("c.ts");
    // Varias carpetas (worktrees) pueden compartir repo: se agrupan por repo_root.
    let sql = format!(
        "SELECT MIN(p.id), p.name, p.repo_root, COALESCE(SUM(x.cost), 0), COALESCE(SUM(x.n), 0)
         FROM projects p LEFT JOIN (
           SELECT s.project_id AS id, SUM(c.cost_usd) AS cost, COUNT(*) AS n
           FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w} GROUP BY 1
         ) x ON x.id = p.id
         WHERE EXISTS (SELECT 1 FROM sessions s WHERE s.project_id = p.id)
         GROUP BY p.repo_root ORDER BY 4 DESC, 5 DESC, p.name"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(ProjectRow { id: r.get(0)?, name: r.get(1)?, cwd: r.get(2)?, cost_usd: r.get(3)?, calls: r.get(4)? })
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
        let by_agent = summary(&conn, &Filter { agents: Some(vec!["codex".into()]), ..Default::default() }, 0).unwrap();
        assert_eq!(by_agent.calls, 1);
        let by_project = summary(&conn, &Filter { projects: Some(vec![2]), ..Default::default() }, 0).unwrap();
        assert!((by_project.cost_usd - 1.0).abs() < 1e-9);
        let by_time = summary(&conn, &Filter { from: Some(1500), to: Some(5000), ..Default::default() }, 0).unwrap();
        assert_eq!(by_time.calls, 2);
        let none = summary(&conn, &Filter { agents: Some(vec![]), ..Default::default() }, 0).unwrap();
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
        let s = summary(&conn, &Filter { from: Some(100000), ..Default::default() }, 100000 + 10 * 60_000).unwrap();
        assert!((s.cache_hit - 0.8).abs() < 1e-9);
        assert!(s.burn_rate_usd_h > 0.0);
        // ahorro: 800 × (3 − 0,3) / 1e6
        assert!((s.cache_savings_usd - 800.0 * 2.7 / 1e6).abs() < 1e-12);
        let idle = summary(&conn, &Filter::default(), 100000 + 2 * HOUR_MS).unwrap();
        assert_eq!(idle.burn_rate_usd_h, 0.0);
    }

    #[test]
    fn serie_diaria_en_hora_local() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        // 23:30 y 00:30 UTC del mismo día en UTC+2 → mismo día local.
        let base = 20 * DAY_MS;
        for (id, ts) in [("d1", base - 30 * 60_000), ("d2", base + 30 * 60_000)] {
            conn.execute(
                "INSERT INTO calls (message_id,session_id,ts,model) VALUES (?1,'s1',?2,'claude-sonnet-4-5')",
                rusqlite::params![id, ts],
            )
            .unwrap();
        }
        let f = Filter { from: Some(base - DAY_MS), ..Default::default() };
        let local = timeseries(&conn, &f, "day", 120).unwrap();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].ts, base - 2 * HOUR_MS, "medianoche local expresada en UTC");
        let utc = timeseries(&conn, &f, "day", 0).unwrap();
        assert_eq!(utc.len(), 2);
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
        let branches = breakdown(&conn, &Filter { projects: Some(vec![1]), ..Default::default() }, "branch").unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].label, "main");
        assert!(breakdown(&conn, &Filter::default(), "nada").is_err());
    }

    #[test]
    fn desglose_por_agente() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let rows = breakdown(&conn, &Filter::default(), "agent").unwrap();
        assert_eq!(rows.iter().map(|r| (r.label.as_str(), r.calls, r.sessions)).collect::<Vec<_>>(), vec![("Claude Code", 3, 2), ("Codex", 1, 1)]);
    }

    #[test]
    fn comandos_agrupados_por_primera_palabra() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        for (i, (tool, target, err)) in
            [("Bash", "git status", 0), ("Bash", "git diff", 1), ("Bash", "npm test", 0), ("Read", "/a.rs", 0)].iter().enumerate()
        {
            conn.execute(
                "INSERT INTO tool_calls (call_id,session_id,ts,tool,target,is_error) VALUES (?1,'s1',1000,?2,?3,?4)",
                rusqlite::params![format!("t{i}"), tool, target, err],
            )
            .unwrap();
        }
        let cmds = breakdown(&conn, &Filter::default(), "command").unwrap();
        assert_eq!(cmds.len(), 2);
        assert_eq!((cmds[0].key.as_str(), cmds[0].calls, cmds[0].errors), ("git", 2, 1));
        assert_eq!((cmds[1].key.as_str(), cmds[1].calls), ("npm", 1));
        let tools = breakdown(&conn, &Filter::default(), "tool").unwrap();
        assert_eq!((tools[0].key.as_str(), tools[0].calls), ("Bash", 3));
    }

    #[test]
    fn listas_de_agentes_y_proyectos() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let agents = list_agents(&conn, &Filter { agents: Some(vec![]), ..Default::default() }).unwrap();
        assert_eq!(agents.len(), 2, "la lista ignora el filtro de agentes");
        assert_eq!(agents[0].id, "claude-code");
        let projects = list_projects(&conn, &Filter { from: Some(8000), ..Default::default() }).unwrap();
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
        let web = summary(&conn, &Filter { projects: Some(vec![1]), ..Default::default() }, 0).unwrap();
        assert_eq!(web.calls, 4, "3 de /w + 1 del worktree");
        let rows = breakdown(&conn, &Filter::default(), "project").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(list_projects(&conn, &Filter::default()).unwrap().len(), 2);
    }

    #[test]
    fn info_de_datos() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let info = data_info(&conn).unwrap();
        assert_eq!((info.first_ts, info.calls, info.watched_files, info.last_scan), (Some(1000), 4, 0, None));
    }
}
