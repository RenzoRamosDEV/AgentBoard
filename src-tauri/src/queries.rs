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
            conds.push(format!("s.project_id IN ({})", placeholders(projects.len())));
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
    pub first_ts: Option<i64>,
    pub last_ts: Option<i64>,
    pub unpriced_models: Vec<String>,
}

pub fn summary(conn: &Connection, f: &Filter) -> Result<Summary> {
    let (w, args) = f.sql("c.ts");
    let sql = format!(
        "SELECT COALESCE(SUM(c.cost_usd),0), COUNT(*), COUNT(DISTINCT c.session_id),
                COALESCE(SUM(c.input_tokens),0), COALESCE(SUM(c.output_tokens),0),
                COALESCE(SUM(c.cache_read),0), COALESCE(SUM(c.cache_write),0),
                MIN(c.ts), MAX(c.ts)
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
            first_ts: r.get(7)?,
            last_ts: r.get(8)?,
            unpriced_models: vec![],
        })
    })?;
    let sql = format!(
        "SELECT DISTINCT c.model FROM call_costs c JOIN sessions s ON s.id = c.session_id
         WHERE {w} AND NOT c.has_price ORDER BY 1"
    );
    let mut stmt = conn.prepare(&sql)?;
    out.unpriced_models = stmt
        .query_map(params_from_iter(args.iter()), |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(out)
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
        let s = summary(&conn, &Filter::default()).unwrap();
        assert_eq!((s.calls, s.sessions), (4, 3));
        // 3 + 1.5 (sonnet) + 1 (haiku) + 0 (sin precio)
        assert!((s.cost_usd - 5.5).abs() < 1e-9);
        assert_eq!(s.unpriced_models, vec!["modelo-x".to_string()]);
    }

    #[test]
    fn filtros_por_agente_proyecto_y_periodo() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let by_agent = summary(&conn, &Filter { agents: Some(vec!["codex".into()]), ..Default::default() }).unwrap();
        assert_eq!(by_agent.calls, 1);
        let by_project = summary(&conn, &Filter { projects: Some(vec![2]), ..Default::default() }).unwrap();
        assert!((by_project.cost_usd - 1.0).abs() < 1e-9);
        let by_time = summary(&conn, &Filter { from: Some(1500), to: Some(5000), ..Default::default() }).unwrap();
        assert_eq!(by_time.calls, 2);
        let none = summary(&conn, &Filter { agents: Some(vec![]), ..Default::default() }).unwrap();
        assert_eq!(none.calls, 0);
    }
}
