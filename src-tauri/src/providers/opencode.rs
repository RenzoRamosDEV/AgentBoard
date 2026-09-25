//! OpenCode guarda todo en SQLite: `~/.local/share/opencode/opencode.db` con `session`,
//! `message` (JSON por mensaje) y `part` (JSON por parte: texto, herramienta, razonamiento…).

use super::{prompt_intent, CallRec, Provider, Record, SessionRec, Source, ToolUseRec, TurnRec};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct OpenCode {
    roots: Option<Vec<PathBuf>>,
}

impl OpenCode {
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self { roots: Some(roots) }
    }
}

/// Nombres de herramienta de OpenCode → los mismos que usa Claude Code, para que la
/// clasificación de actividad y el 1-shot funcionen igual.
fn canonical_tool(name: &str) -> String {
    match name {
        "bash" => "Bash",
        "edit" => "Edit",
        "write" => "Write",
        "read" => "Read",
        "glob" => "Glob",
        "grep" => "Grep",
        "list" => "LS",
        "webfetch" => "WebFetch",
        "websearch" => "WebSearch",
        "task" => "Agent",
        "skill" => "Skill",
        "question" => "AskUserQuestion",
        "todowrite" => "TodoWrite",
        "todoread" => "TodoRead",
        "patch" | "apply_patch" => "Edit",
        other => return other.to_string(),
    }
    .to_string()
}

impl Provider for OpenCode {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn name(&self) -> &'static str {
        "OpenCode"
    }

    fn source(&self) -> Source {
        Source::Sqlite
    }

    fn log_roots(&self) -> Vec<PathBuf> {
        if let Some(r) = &self.roots {
            return r.clone();
        }
        dirs::data_dir()
            .map(|d| vec![d.join("opencode")])
            .unwrap_or_default()
    }

    fn installed(&self) -> bool {
        self.log_roots().iter().any(|r| r.is_dir()) || super::on_path("opencode")
    }

    fn matches(&self, path: &Path) -> bool {
        path.file_name().is_some_and(|f| f == "opencode.db")
            && self.log_roots().iter().any(|r| path.starts_with(r))
    }

    fn parse_line(&self, _path: &Path, _line: &str) -> Result<Vec<Record>> {
        Ok(vec![])
    }

    fn read_db(&self, path: &Path, since: i64) -> Result<(Vec<Record>, i64)> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("no se pudo abrir {}", path.display()))?;
        let mut out = Vec::new();
        let mut cursor = since;

        // Sesiones: carpeta y si son hijas (subagentes). Se cargan todas: son pocas y hacen
        // falta para resolver las de los mensajes nuevos.
        let mut sessions: HashMap<String, (String, bool, i64)> = HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT id, directory, parent_id IS NOT NULL, time_created, time_updated FROM session",
        )?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })? {
            let (id, dir, child, created, updated) = row?;
            sessions.insert(id.clone(), (dir.clone(), child, created));
            if updated > since {
                cursor = cursor.max(updated);
                out.push(Record::Session(SessionRec {
                    id,
                    cwd: Some(dir),
                    git_branch: None,
                    ts: created,
                    is_subagent: child,
                }));
            }
        }

        // Mensajes nuevos o actualizados: assistant → llamada, user → turno.
        let mut stmt = conn.prepare("SELECT id, session_id, time_created, time_updated, data FROM message WHERE time_updated > ?1 ORDER BY time_created")?;
        let mut text_parts = conn.prepare("SELECT data FROM part WHERE message_id = ?1")?;
        for row in stmt.query_map(params![since], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })? {
            let (id, session_id, created, updated, data) = row?;
            cursor = cursor.max(updated);
            let Ok(d) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            let Some((dir, child, s_created)) = sessions.get(&session_id) else {
                continue;
            };
            out.push(Record::Session(SessionRec {
                id: session_id.clone(),
                cwd: Some(dir.clone()),
                git_branch: None,
                ts: created.min(*s_created),
                is_subagent: *child,
            }));
            match d["role"].as_str() {
                Some("assistant") => {
                    let model = format!(
                        "{}/{}",
                        d["providerID"].as_str().unwrap_or(""),
                        d["modelID"].as_str().unwrap_or("")
                    );
                    let t = &d["tokens"];
                    let n = |v: &Value| v.as_i64().unwrap_or(0);
                    out.push(Record::Call(CallRec {
                        message_id: id,
                        session_id: session_id.clone(),
                        ts: d["time"]["created"].as_i64().unwrap_or(created),
                        model: crate::pricing::normalize_model(model.trim_start_matches('/')),
                        input_tokens: n(&t["input"]),
                        output_tokens: n(&t["output"]) + n(&t["reasoning"]),
                        cache_read: n(&t["cache"]["read"]),
                        cache_write: n(&t["cache"]["write"]),
                        cache_write_1h: 0,
                        reasoning_tokens: n(&t["reasoning"]),
                        activity: None,
                        is_sidechain: *child,
                        agent_id: child.then(|| session_id.clone()),
                        cost_reported: d["cost"].as_f64(),
                    }));
                }
                Some("user") if !*child => {
                    // Intención por el texto del prompt; el texto no se guarda.
                    let mut text = String::new();
                    for p in text_parts.query_map(params![&id], |r| r.get::<_, String>(0))? {
                        if let Ok(v) = serde_json::from_str::<Value>(&p?) {
                            if v["type"] == "text" {
                                text.push_str(v["text"].as_str().unwrap_or(""));
                            }
                        }
                    }
                    out.push(Record::Turn(TurnRec {
                        id,
                        session_id: session_id.clone(),
                        ts: created,
                        intent: prompt_intent(&text),
                    }));
                }
                _ => {}
            }
        }

        // Partes de herramienta nuevas o actualizadas.
        let mut stmt = conn.prepare("SELECT id, message_id, session_id, time_created, time_updated, data FROM part WHERE time_updated > ?1 ORDER BY time_created")?;
        for row in stmt.query_map(params![since], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
            ))
        })? {
            let (id, message_id, session_id, created, updated, data) = row?;
            cursor = cursor.max(updated);
            let Ok(d) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            if d["type"] != "tool" {
                continue;
            }
            let raw = d["tool"].as_str().unwrap_or("?");
            let tool = canonical_tool(raw);
            let input = &d["state"]["input"];
            let target = [
                "command",
                "filePath",
                "file_path",
                "path",
                "pattern",
                "query",
                "url",
                "description",
            ]
            .iter()
            .find_map(|k| input[*k].as_str())
            .map(|s| s.chars().take(500).collect::<String>());
            let detail = match tool.as_str() {
                "Agent" => Some(
                    input["subagent_type"]
                        .as_str()
                        .or(input["description"].as_str())
                        .unwrap_or("general-purpose")
                        .to_string(),
                ),
                "Skill" => input["name"]
                    .as_str()
                    .or(input["skill"].as_str())
                    .map(str::to_string),
                _ => None,
            };
            let ts = d["state"]["time"]["start"].as_i64().unwrap_or(created);
            let end = d["state"]["time"]["end"].as_i64();
            let is_error = d["state"]["status"] == "error";
            out.push(Record::ToolUse(ToolUseRec {
                call_id: d["callID"].as_str().unwrap_or(&id).to_string(),
                message_id,
                session_id,
                ts,
                tool,
                target,
                detail,
            }));
            if let Some(end) = end {
                out.push(Record::ToolResult {
                    call_id: d["callID"].as_str().unwrap_or(&id).to_string(),
                    ts: end,
                    is_error,
                    agent_id: d["state"]["metadata"]["sessionId"]
                        .as_str()
                        .map(str::to_string),
                });
            }
        }
        Ok((out, cursor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normaliza_herramientas() {
        assert_eq!(canonical_tool("bash"), "Bash");
        assert_eq!(canonical_tool("edit"), "Edit");
        assert_eq!(canonical_tool("task"), "Agent");
        assert_eq!(canonical_tool("mcp_figma_get_file"), "mcp_figma_get_file");
    }

    #[test]
    fn detecta_la_base() {
        let p = OpenCode::with_roots(vec![PathBuf::from("/d/opencode")]);
        assert!(p.matches(Path::new("/d/opencode/opencode.db")));
        assert!(!p.matches(Path::new("/d/opencode/opencode.db-wal")));
        assert!(!p.matches(Path::new("/otro/opencode.db")));
    }
}
