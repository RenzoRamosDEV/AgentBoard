//! Apartados al estilo CodeBurn que necesitan lógica en Rust: actividad por turno con 1-shot,
//! comandos de shell separados por tuberías, skills, servidores MCP y tipos de subagente.

use crate::queries::{BreakdownRow, Filter};
use anyhow::Result;
use rusqlite::{params_from_iter, Connection};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

const SHELL_TOOLS: &str = "('Bash', 'shell', 'exec_command', 'run_shell_command')";
const EDIT_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit", "apply_patch", "replace", "write_file"];

// ---------------------------------------------------------------------------
// Comandos de shell

/// Primera palabra de cada comando de una línea de shell: separa `|`, `&&`, `||`, `;` y saltos
/// de línea fuera de comillas, ignora el cuerpo de los heredocs y salta prefijos como `sudo`.
pub fn split_commands(line: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut cur = String::new();
    let (mut single, mut double) = (false, false);
    let mut heredoc: Option<String> = None;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            '<' if !single && !double && chars.get(i + 1) == Some(&'<') && chars.get(i + 2) != Some(&'<') => {
                // `<<EOF`, `<<'EOF'`, `<<-EOF`: guarda el delimitador y sigue con la línea.
                let rest: String = chars[i + 2..].iter().collect();
                let word: String = rest
                    .trim_start_matches('-')
                    .trim_start()
                    .chars()
                    .take_while(|c| !c.is_whitespace() && !";|&)".contains(*c))
                    .collect();
                let word = word.trim_matches(|c| c == '\'' || c == '"').to_string();
                if !word.is_empty() {
                    heredoc = Some(word);
                }
            }
            '\n' if !single && !double => {
                segments.push(std::mem::take(&mut cur));
                if let Some(delim) = heredoc.take() {
                    // Salta el cuerpo hasta la línea que es solo el delimitador.
                    let mut j = i + 1;
                    loop {
                        let end = chars[j.min(chars.len())..].iter().position(|c| *c == '\n').map(|p| j + p).unwrap_or(chars.len());
                        let l: String = chars[j.min(chars.len())..end].iter().collect();
                        j = end + 1;
                        if l.trim() == delim || end >= chars.len() {
                            break;
                        }
                    }
                    i = j;
                    continue;
                }
                i += 1;
                continue;
            }
            // `&` de redirección (`2>&1`, `&>`) no separa comandos.
            '&' if i > 0 && matches!(chars[i - 1], '>' | '<') || chars.get(i + 1) == Some(&'>') => {}
            '|' | ';' | '&' if !single && !double => {
                segments.push(std::mem::take(&mut cur));
                i += 1;
                continue;
            }
            _ => {}
        }
        cur.push(c);
        i += 1;
    }
    segments.push(cur);
    segments.iter().filter_map(|s| command_name(s)).collect()
}

fn command_name(segment: &str) -> Option<String> {
    const SKIP: &[&str] = &["sudo", "env", "time", "nohup", "exec", "command", "builtin", "then", "do", "else", "!"];
    const NOT_COMMANDS: &[&str] = &["fi", "done", "esac", "{", "}", "(", ")", "in"];
    let mut tokens = segment.split_whitespace().peekable();
    while let Some(tok) = tokens.next() {
        let tok = tok.trim_start_matches(['(', '{', '$']).trim_end_matches([')', '}']);
        if tok.is_empty() || SKIP.contains(&tok) {
            continue;
        }
        // Asignaciones `VAR=valor` antes del comando.
        if let Some((name, _)) = tok.split_once('=') {
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
        }
        if tok == "timeout" {
            tokens.next_if(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit()));
            continue;
        }
        if NOT_COMMANDS.contains(&tok) || tok.starts_with('-') || tok.starts_with('#') || tok.starts_with('>') {
            return None;
        }
        let name = tok.rsplit('/').next().unwrap_or(tok);
        return (!name.is_empty()).then(|| name.to_string());
    }
    None
}

pub fn shell_commands(conn: &Connection, f: &Filter) -> Result<Vec<BreakdownRow>> {
    let (w, args) = f.sql("t.ts");
    let mut stmt = conn.prepare(&format!(
        "SELECT t.target, t.is_error FROM tool_calls t JOIN sessions s ON s.id = t.session_id
         WHERE {w} AND t.target IS NOT NULL AND t.tool IN {SHELL_TOOLS}"
    ))?;
    let mut counts: HashMap<String, (i64, i64)> = HashMap::new();
    let mut rows = stmt.query(params_from_iter(args.iter()))?;
    while let Some(r) = rows.next()? {
        let target: String = r.get(0)?;
        let is_error: bool = r.get(1)?;
        for cmd in split_commands(&target) {
            let e = counts.entry(cmd).or_default();
            e.0 += 1;
            e.1 += is_error as i64;
        }
    }
    Ok(sorted(counts.into_iter().map(|(k, (n, e))| BreakdownRow::simple(k, n, e, 0.0)).collect(), 50))
}

fn sorted(mut rows: Vec<BreakdownRow>, limit: usize) -> Vec<BreakdownRow> {
    rows.sort_by(|a, b| b.calls.cmp(&a.calls).then(b.cost_usd.total_cmp(&a.cost_usd)).then(a.key.cmp(&b.key)));
    rows.truncate(limit);
    rows
}

// ---------------------------------------------------------------------------
// Skills, MCP y subagentes

/// Cada skill o tipo de subagente invocado, con sus usos y el coste de la respuesta que lo invoca.
pub fn skills_and_agents(conn: &Connection, f: &Filter) -> Result<Vec<BreakdownRow>> {
    let (w, args) = f.sql("t.ts");
    let mut stmt = conn.prepare(&format!(
        "SELECT t.detail, COUNT(*), SUM(t.is_error),
                SUM((SELECT COALESCE(SUM(c.cost_usd), 0) FROM call_costs c WHERE c.message_id = t.message_id))
         FROM tool_calls t JOIN sessions s ON s.id = t.session_id
         WHERE {w} AND t.tool IN ('Skill', 'Agent', 'Task') AND t.detail IS NOT NULL
         GROUP BY 1"
    ))?;
    let rows = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(BreakdownRow::simple(r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(sorted(rows, 50))
}

/// Llamadas por servidor MCP (`mcp__<servidor>__<herramienta>`).
pub fn mcp_servers(conn: &Connection, f: &Filter) -> Result<Vec<BreakdownRow>> {
    let (w, args) = f.sql("t.ts");
    let mut stmt = conn.prepare(&format!(
        r"SELECT t.tool, COUNT(*), SUM(t.is_error) FROM tool_calls t JOIN sessions s ON s.id = t.session_id
          WHERE {w} AND t.tool LIKE 'mcp\_\_%' ESCAPE '\' GROUP BY 1"
    ))?;
    let mut by_server: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut rows = stmt.query(params_from_iter(args.iter()))?;
    while let Some(r) = rows.next()? {
        let tool: String = r.get(0)?;
        let server = tool.split("__").nth(1).unwrap_or(&tool).to_string();
        let e = by_server.entry(server).or_default();
        e.0 += r.get::<_, i64>(1)?;
        e.1 += r.get::<_, i64>(2)?;
    }
    Ok(sorted(by_server.into_iter().map(|(k, (n, e))| BreakdownRow::simple(k, n, e, 0.0)).collect(), 50))
}

/// Llamadas hechas dentro de subagentes, por tipo de subagente.
pub fn agent_types(conn: &Connection, f: &Filter) -> Result<Vec<BreakdownRow>> {
    let (w, args) = f.sql("c.ts");
    let mut stmt = conn.prepare(&format!(
        "SELECT COALESCE(a.detail, 'desconocido'), COUNT(*), SUM(c.cost_usd)
         FROM call_costs c JOIN sessions s ON s.id = c.session_id
         LEFT JOIN (SELECT agent_id, MIN(detail) AS detail FROM tool_calls
                    WHERE agent_id IS NOT NULL GROUP BY agent_id) a ON a.agent_id = c.agent_id
         WHERE {w} AND c.is_sidechain = 1 GROUP BY 1"
    ))?;
    let rows = stmt
        .query_map(params_from_iter(args.iter()), |r| Ok(BreakdownRow::simple(r.get::<_, String>(0)?, r.get(1)?, 0, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut rows: Vec<BreakdownRow> = rows;
    rows.sort_by(|a, b| b.cost_usd.total_cmp(&a.cost_usd));
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Actividad por turno y 1-shot

#[derive(Debug, Default)]
pub struct TurnStats {
    pub ts: i64,
    pub intent: Option<String>,
    pub cost_usd: f64,
    /// Llamadas por modelo, para saber el modelo dominante del turno.
    pub models: HashMap<String, i64>,
    /// (herramienta, objetivo, error)
    pub tools: Vec<(String, Option<String>, bool)>,
}

impl TurnStats {
    fn edits(&self) -> impl Iterator<Item = &(String, Option<String>, bool)> {
        self.tools.iter().filter(|(t, _, _)| EDIT_TOOLS.contains(&t.as_str()))
    }

    fn shell(&self) -> impl Iterator<Item = &str> {
        self.tools
            .iter()
            .filter(|(t, _, _)| matches!(t.as_str(), "Bash" | "shell" | "exec_command" | "run_shell_command"))
            .filter_map(|(_, target, _)| target.as_deref())
    }

    pub fn has_edits(&self) -> bool {
        self.edits().next().is_some()
    }

    /// Ninguna edición falló y ningún archivo se editó dos veces.
    pub fn is_one_shot(&self) -> bool {
        let mut seen = HashSet::new();
        self.edits().all(|(_, target, err)| !err && seen.insert(target.clone()))
    }

    pub fn dominant_model(&self) -> Option<&str> {
        self.models.iter().max_by_key(|(m, n)| (**n, std::cmp::Reverse((*m).clone()))).map(|(m, _)| m.as_str())
    }
}

fn shell_matches(cmd: &str, keys: &[&str]) -> bool {
    let c = cmd.to_ascii_lowercase();
    keys.iter().any(|k| c.contains(k))
}

const TEST_KEYS: &[&str] = &["test", "pytest", "jest", "vitest", "mocha", "rspec", "cargo t ", "go t "];
const BUILD_KEYS: &[&str] = &[
    "build", "deploy", "docker", "podman", "kubectl", "terraform", "make ", "cmake", "gradle", "mvn ", "compile", "release",
    "publish", "npm i", "npm ci", "pip install", "cargo check",
];

/// Actividad de un turno según sus herramientas y la intención de su prompt.
pub fn classify_turn(t: &TurnStats) -> &'static str {
    let intent = t.intent.as_deref();
    if t.has_edits() {
        return match intent {
            Some("debug") => "debugging",
            Some("feature") => "feature",
            _ => "coding",
        };
    }
    let shell: Vec<&str> = t.shell().collect();
    if shell.iter().any(|c| shell_matches(c, TEST_KEYS)) {
        return "testing";
    }
    if shell.iter().any(|c| shell_matches(c, BUILD_KEYS)) {
        return "build";
    }
    if !shell.is_empty() && shell.iter().all(|c| split_commands(c).iter().all(|w| w == "git" || w == "gh")) {
        return "git";
    }
    if t.tools.iter().any(|(tool, _, _)| tool == "Agent" || tool == "Task") {
        return "delegation";
    }
    if !t.tools.is_empty() {
        return "exploration";
    }
    if intent == Some("brainstorm") {
        "brainstorming"
    } else {
        "conversation"
    }
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRow {
    pub key: String,
    pub cost_usd: f64,
    pub turns: i64,
    pub edit_turns: i64,
    /// Solo para actividades con ediciones.
    pub one_shot: Option<f64>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelOneShot {
    pub model: String,
    pub edit_turns: i64,
    pub one_shot: Option<f64>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActivityReport {
    pub activities: Vec<ActivityRow>,
    pub models: Vec<ModelOneShot>,
}

pub fn turn_stats(conn: &Connection, f: &Filter) -> Result<HashMap<String, TurnStats>> {
    let mut turns: HashMap<String, TurnStats> = HashMap::new();
    let (w, args) = f.sql("t.ts");
    let mut stmt = conn.prepare(&format!("SELECT t.id, t.intent, t.ts FROM turns t JOIN sessions s ON s.id = t.session_id WHERE {w}"))?;
    let mut rows = stmt.query(params_from_iter(args.iter()))?;
    while let Some(r) = rows.next()? {
        let t = turns.entry(r.get(0)?).or_default();
        t.intent = r.get(1)?;
        t.ts = r.get(2)?;
    }

    let (w, args) = f.sql("c.ts");
    let mut stmt = conn.prepare(&format!(
        "SELECT c.turn_id, c.model, SUM(c.cost_usd), COUNT(*) FROM call_costs c JOIN sessions s ON s.id = c.session_id
         WHERE {w} AND c.turn_id IS NOT NULL GROUP BY 1, 2"
    ))?;
    let mut rows = stmt.query(params_from_iter(args.iter()))?;
    while let Some(r) = rows.next()? {
        let t = turns.entry(r.get(0)?).or_default();
        t.cost_usd += r.get::<_, f64>(2)?;
        *t.models.entry(r.get(1)?).or_default() += r.get::<_, i64>(3)?;
    }

    let (w, args) = f.sql("t.ts");
    let mut stmt = conn.prepare(&format!(
        "SELECT t.turn_id, t.tool, t.target, t.is_error FROM tool_calls t JOIN sessions s ON s.id = t.session_id
         WHERE {w} AND t.turn_id IS NOT NULL"
    ))?;
    let mut rows = stmt.query(params_from_iter(args.iter()))?;
    while let Some(r) = rows.next()? {
        turns.entry(r.get(0)?).or_default().tools.push((r.get(1)?, r.get(2)?, r.get(3)?));
    }
    Ok(turns)
}

pub fn activity(conn: &Connection, f: &Filter) -> Result<ActivityReport> {
    let turns = turn_stats(conn, f)?;
    #[derive(Default)]
    struct Acc {
        cost: f64,
        turns: i64,
        edit_turns: i64,
        one_shot: i64,
    }
    let mut by_activity: HashMap<String, Acc> = HashMap::new();
    let mut by_model: HashMap<String, Acc> = HashMap::new();
    for t in turns.values() {
        let a = by_activity.entry(classify_turn(t).to_string()).or_default();
        a.cost += t.cost_usd;
        a.turns += 1;
        if t.has_edits() {
            a.edit_turns += 1;
            a.one_shot += t.is_one_shot() as i64;
            if let Some(m) = t.dominant_model() {
                let m = by_model.entry(m.to_string()).or_default();
                m.edit_turns += 1;
                m.one_shot += t.is_one_shot() as i64;
            }
        }
    }

    // Llamadas sin turno (agentes que no registran prompts): por su actividad de llamada.
    let (w, args) = f.sql("c.ts");
    let mut stmt = conn.prepare(&format!(
        "SELECT COALESCE(c.activity, 'conversation'), SUM(c.cost_usd) FROM call_costs c JOIN sessions s ON s.id = c.session_id
         WHERE {w} AND c.turn_id IS NULL GROUP BY 1"
    ))?;
    let mut rows = stmt.query(params_from_iter(args.iter()))?;
    while let Some(r) = rows.next()? {
        by_activity.entry(r.get(0)?).or_default().cost += r.get::<_, f64>(1)?;
    }

    let rate = |a: &Acc| (a.edit_turns > 0).then(|| a.one_shot as f64 / a.edit_turns as f64);
    let mut activities: Vec<ActivityRow> = by_activity
        .iter()
        .map(|(k, a)| ActivityRow { key: k.clone(), cost_usd: a.cost, turns: a.turns, edit_turns: a.edit_turns, one_shot: rate(a) })
        .collect();
    activities.sort_by(|a, b| b.cost_usd.total_cmp(&a.cost_usd).then(b.turns.cmp(&a.turns)).then(a.key.cmp(&b.key)));
    let mut models: Vec<ModelOneShot> = by_model
        .iter()
        .map(|(k, a)| ModelOneShot { model: k.clone(), edit_turns: a.edit_turns, one_shot: rate(a) })
        .collect();
    models.sort_by(|a, b| a.model.cmp(&b.model));
    Ok(ActivityReport { activities, models })
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDay {
    /// Inicio del día local en epoch ms UTC.
    pub ts: i64,
    pub activity: String,
    pub cost_usd: f64,
    pub turns: i64,
}

/// Coste y turnos por día local y actividad (para el gráfico apilado).
pub fn activity_daily(conn: &Connection, f: &Filter, tz_offset_min: i64) -> Result<Vec<ActivityDay>> {
    const DAY_MS: i64 = 86_400_000;
    let off = tz_offset_min * 60_000;
    let mut acc: BTreeMap<(i64, String), (f64, i64)> = BTreeMap::new();
    for t in turn_stats(conn, f)?.values() {
        if t.ts == 0 {
            continue; // llamadas sin turno registrado
        }
        let day = ((t.ts + off) / DAY_MS) * DAY_MS - off;
        let e = acc.entry((day, classify_turn(t).to_string())).or_default();
        e.0 += t.cost_usd;
        e.1 += 1;
    }
    Ok(acc
        .into_iter()
        .map(|((ts, activity), (cost_usd, turns))| ActivityDay { ts, activity, cost_usd, turns })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(intent: Option<&str>, tools: &[(&str, &str, bool)]) -> TurnStats {
        TurnStats {
            intent: intent.map(str::to_string),
            tools: tools.iter().map(|(t, x, e)| (t.to_string(), Some(x.to_string()), *e)).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn separa_tuberias_y_encadenamientos() {
        assert_eq!(split_commands("grep -r foo . | head -5 && git status"), vec!["grep", "head", "git"]);
        assert_eq!(split_commands("cd /x; FOO=1 sudo npm test || echo 'a | b'"), vec!["cd", "npm", "echo"]);
        assert_eq!(split_commands("timeout 30 /usr/bin/python3 x.py 2>&1 | tail -3"), vec!["python3", "tail"]);
    }

    #[test]
    fn ignora_el_cuerpo_de_los_heredocs() {
        let cmd = "cat > a.rs <<'EOF'\nfn main() { x | y; }\nEOF\ncargo build 2>&1 | tail -2";
        assert_eq!(split_commands(cmd), vec!["cat", "cargo", "tail"]);
        let py = "python3 - <<EOF\nimport os\nEOF";
        assert_eq!(split_commands(py), vec!["python3"]);
    }

    #[test]
    fn clasifica_turnos() {
        assert_eq!(classify_turn(&turn(Some("debug"), &[("Edit", "a.rs", false)])), "debugging");
        assert_eq!(classify_turn(&turn(Some("feature"), &[("Write", "a.rs", false)])), "feature");
        assert_eq!(classify_turn(&turn(None, &[("Edit", "a.rs", false)])), "coding");
        assert_eq!(classify_turn(&turn(None, &[("Bash", "cargo test", false)])), "testing");
        assert_eq!(classify_turn(&turn(None, &[("Bash", "npm run build", false)])), "build");
        assert_eq!(classify_turn(&turn(None, &[("Bash", "git status && git diff", false)])), "git");
        assert_eq!(classify_turn(&turn(None, &[("Agent", "x", false)])), "delegation");
        assert_eq!(classify_turn(&turn(None, &[("Read", "a.rs", false)])), "exploration");
        assert_eq!(classify_turn(&turn(Some("brainstorm"), &[])), "brainstorming");
        assert_eq!(classify_turn(&turn(None, &[])), "conversation");
    }

    #[test]
    fn one_shot() {
        assert!(turn(None, &[("Edit", "a.rs", false), ("Edit", "b.rs", false)]).is_one_shot());
        assert!(!turn(None, &[("Edit", "a.rs", false), ("Bash", "cargo test", true), ("Edit", "a.rs", false)]).is_one_shot());
        assert!(!turn(None, &[("Edit", "a.rs", true)]).is_one_shot());
    }
}
