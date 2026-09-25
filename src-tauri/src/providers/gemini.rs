//! Gemini CLI: `~/.gemini/tmp/<proyecto>/chats/session-<fecha>-<id>.jsonl` (`GEMINI_CLI_HOME` lo cambia).
//! Primera línea: metadatos `{sessionId, projectHash, startTime, kind, directories?}`; después un
//! mensaje por línea (`type: user | gemini | info…`, con `tokens`, `model` y `toolCalls` en los de
//! Gemini) y actualizaciones `{"$set": {...}}`. Los subagentes van en `chats/<sesión padre>/<id>.jsonl`.
//! Las versiones antiguas escribían un único `.json` con el mismo contenido.

use super::{parse_ts, prompt_intent, CallRec, Provider, Record, SessionRec, ToolUseRec, TurnRec};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
pub struct Gemini {
    roots: Option<Vec<PathBuf>>,
    state: Mutex<HashMap<PathBuf, FileState>>,
}

#[derive(Default)]
struct FileState {
    session_id: String,
    cwd: Option<String>,
    is_subagent: bool,
    turns: u32,
    /// Último modelo visto, para mensajes sin él.
    model: String,
}

impl Gemini {
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self {
            roots: Some(roots),
            state: Mutex::default(),
        }
    }

    fn home() -> Option<PathBuf> {
        match std::env::var_os("GEMINI_CLI_HOME") {
            Some(h) if !h.is_empty() => Some(PathBuf::from(h).join(".gemini")),
            _ => dirs::home_dir().map(|h| h.join(".gemini")),
        }
    }

    fn apply_meta(st: &mut FileState, meta: &Value) {
        if let Some(id) = meta["sessionId"].as_str() {
            st.session_id = id.to_string();
        }
        if let Some(dir) = meta["directories"]
            .as_array()
            .and_then(|d| d.first())
            .and_then(|d| d.as_str())
        {
            st.cwd = Some(dir.to_string());
        }
        if meta["kind"].as_str() == Some("subagent") {
            st.is_subagent = true;
        }
    }

    fn records_for_message(st: &mut FileState, m: &Value, out: &mut Vec<Record>) {
        let Some(ts) = m["timestamp"].as_str().and_then(parse_ts) else {
            return;
        };
        let id = m["id"].as_str().unwrap_or("").to_string();
        match m["type"].as_str() {
            Some("user") if !st.is_subagent => {
                st.turns += 1;
                let text = text_of(&m["content"]);
                out.push(Record::Turn(TurnRec {
                    id: if id.is_empty() {
                        format!("{}:{}", st.session_id, st.turns)
                    } else {
                        id
                    },
                    session_id: st.session_id.clone(),
                    ts,
                    intent: prompt_intent(&text),
                }));
            }
            Some("gemini") => {
                if let Some(model) = m["model"].as_str() {
                    st.model = model.to_string();
                }
                let t = &m["tokens"];
                if t.is_object() && !id.is_empty() {
                    let n = |k: &str| t[k].as_i64().unwrap_or(0);
                    let cached = n("cached");
                    out.push(Record::Call(CallRec {
                        message_id: id.clone(),
                        session_id: st.session_id.clone(),
                        ts,
                        model: crate::pricing::normalize_model(&st.model),
                        // `input` (promptTokenCount) incluye lo servido desde caché.
                        input_tokens: (n("input") - cached).max(0),
                        output_tokens: n("output") + n("thoughts"),
                        cache_read: cached,
                        cache_write: 0,
                        cache_write_1h: 0,
                        reasoning_tokens: n("thoughts"),
                        activity: None,
                        is_sidechain: st.is_subagent,
                        agent_id: st.is_subagent.then(|| st.session_id.clone()),
                        cost_reported: None,
                    }));
                }
                if let Some(calls) = m["toolCalls"].as_array() {
                    for c in calls {
                        let Some(call_id) = c["id"].as_str() else {
                            continue;
                        };
                        let raw = c["name"].as_str().unwrap_or("?");
                        let tool = canonical_tool(raw).to_string();
                        let args = &c["args"];
                        let target = [
                            "command",
                            "file_path",
                            "absolute_path",
                            "path",
                            "pattern",
                            "query",
                            "url",
                            "prompt",
                        ]
                        .iter()
                        .find_map(|k| args[*k].as_str())
                        .map(|s| s.chars().take(500).collect::<String>());
                        let detail = match tool.as_str() {
                            "Agent" => Some(
                                args["subagent_type"]
                                    .as_str()
                                    .or(args["agent"].as_str())
                                    .or(args["name"].as_str())
                                    .unwrap_or("general-purpose")
                                    .to_string(),
                            ),
                            "Skill" => args["name"]
                                .as_str()
                                .or(args["skill"].as_str())
                                .map(str::to_string),
                            _ => None,
                        };
                        let cts = c["timestamp"].as_str().and_then(parse_ts).unwrap_or(ts);
                        out.push(Record::ToolUse(ToolUseRec {
                            call_id: call_id.to_string(),
                            message_id: id.clone(),
                            session_id: st.session_id.clone(),
                            ts: cts,
                            tool,
                            target,
                            detail,
                        }));
                        if let Some(status) = c["status"].as_str() {
                            if status != "executing"
                                && status != "scheduled"
                                && status != "validating"
                                && status != "awaiting_approval"
                            {
                                out.push(Record::ToolResult {
                                    call_id: call_id.to_string(),
                                    ts: cts,
                                    is_error: matches!(status, "error" | "cancelled"),
                                    agent_id: c["agentId"].as_str().map(str::to_string),
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Texto plano de un `content` de Gemini: cadena o lista de partes `{text}`.
fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str().or(p.as_str()))
            .collect::<Vec<_>>()
            .join(" "),
        Value::Object(_) => content["text"].as_str().unwrap_or("").to_string(),
        _ => String::new(),
    }
}

/// Nombres de herramienta de Gemini CLI → los de Claude Code.
fn canonical_tool(name: &str) -> &str {
    match name {
        "run_shell_command" | "shell" => "Bash",
        "read_file" | "read_many_files" => "Read",
        "write_file" => "Write",
        "replace" | "edit" | "smart_edit" => "Edit",
        "search_file_content" | "grep_search" | "grep" => "Grep",
        "glob" | "list_directory" => "Glob",
        "web_fetch" => "WebFetch",
        "google_web_search" | "web_search" => "WebSearch",
        "write_todos" | "save_memory" => "TodoWrite",
        "delegate_to_agent" | "subagent" | "task" => "Agent",
        "activate_skill" | "skill" => "Skill",
        "ask_user" => "AskUserQuestion",
        other => other,
    }
}

impl Provider for Gemini {
    fn id(&self) -> &'static str {
        "gemini"
    }

    fn name(&self) -> &'static str {
        "Gemini CLI"
    }

    fn log_roots(&self) -> Vec<PathBuf> {
        if let Some(r) = &self.roots {
            return r.clone();
        }
        Self::home()
            .map(|h| vec![h.join("tmp")])
            .unwrap_or_default()
    }

    fn installed(&self) -> bool {
        Self::home().is_some_and(|h| h.is_dir()) || super::on_path("gemini")
    }

    fn matches(&self, path: &Path) -> bool {
        let in_chats = path.components().any(|c| c.as_os_str() == "chats");
        let ext = path.extension().and_then(|e| e.to_str());
        in_chats
            && matches!(ext, Some("jsonl") | Some("json"))
            && self.log_roots().iter().any(|r| path.starts_with(r))
    }

    fn reset(&self, path: &Path) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(path);
    }

    fn parse_line(&self, path: &Path, line: &str) -> Result<Vec<Record>> {
        let v: Value = serde_json::from_str(line)?;
        let mut states = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let st = states.entry(path.to_path_buf()).or_default();
        let mut out = Vec::new();

        if let Some(set) = v.get("$set") {
            Self::apply_meta(st, set);
        } else if v.get("messages").is_some() {
            // Formato antiguo: todo el registro en un solo `.json`.
            Self::apply_meta(st, &v);
            for m in v["messages"].as_array().into_iter().flatten() {
                Self::records_for_message(st, m, &mut out);
            }
        } else if v.get("sessionId").is_some() && v.get("type").is_none() {
            Self::apply_meta(st, &v);
        } else if v.get("type").is_some() {
            Self::records_for_message(st, &v, &mut out);
        }

        if !out.is_empty() && !st.session_id.is_empty() {
            let ts = out
                .iter()
                .find_map(|r| match r {
                    Record::Call(c) => Some(c.ts),
                    Record::Turn(t) => Some(t.ts),
                    Record::ToolUse(t) => Some(t.ts),
                    _ => None,
                })
                .unwrap_or(0);
            out.insert(
                0,
                Record::Session(SessionRec {
                    id: st.session_id.clone(),
                    cwd: st.cwd.clone(),
                    git_branch: None,
                    ts,
                    is_subagent: st.is_subagent,
                }),
            );
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecta_sesiones() {
        let p = Gemini::with_roots(vec![PathBuf::from("/h/.gemini/tmp")]);
        assert!(p.matches(Path::new(
            "/h/.gemini/tmp/abc/chats/session-2026-09-25T10-00-1234abcd.jsonl"
        )));
        assert!(p.matches(Path::new("/h/.gemini/tmp/abc/chats/parent/child.jsonl")));
        assert!(!p.matches(Path::new("/h/.gemini/tmp/abc/logs.json")));
    }

    #[test]
    fn texto_de_partes() {
        assert_eq!(
            text_of(&serde_json::json!([{ "text": "hola" }, { "text": "mundo" }])),
            "hola mundo"
        );
        assert_eq!(text_of(&serde_json::json!("hola")), "hola");
    }
}
