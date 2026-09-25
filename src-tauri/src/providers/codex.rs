//! Codex CLI: `~/.codex/sessions/YYYY/MM/DD/rollout-<fecha>-<thread>.jsonl` (y `archived_sessions/`).
//! Cada línea es `{"timestamp","type","payload"}`; el tipo va en `type`: `session_meta`,
//! `turn_context`, `token_usage_record`, `response_item`, `event_msg`, `compacted`…
//!
//! A diferencia de Claude Code, el modelo y la carpeta no van en cada línea: se guardan por
//! archivo en `state` mientras se lee.

use super::{parse_ts, prompt_intent, CallRec, EventRec, Provider, Record, SessionRec, ToolUseRec, TurnRec};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
pub struct Codex {
    roots: Option<Vec<PathBuf>>,
    state: Mutex<HashMap<PathBuf, FileState>>,
}

/// Lo que se sabe de una sesión tras leer sus primeras líneas.
#[derive(Default, Clone)]
struct FileState {
    thread_id: String,
    cwd: Option<String>,
    branch: Option<String>,
    model: String,
    is_subagent: bool,
    turn_id: Option<String>,
    /// Texto del último prompt humano, a la espera de su `turn_context` (no se guarda).
    pending_prompt: Option<String>,
    /// Con `token_usage_record` presentes se ignoran los `token_count` (formato antiguo).
    has_usage_records: bool,
}

impl Codex {
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self { roots: Some(roots), state: Mutex::default() }
    }

    fn home() -> Option<PathBuf> {
        match std::env::var_os("CODEX_HOME") {
            Some(h) if !h.is_empty() => Some(PathBuf::from(h)),
            _ => dirs::home_dir().map(|h| h.join(".codex")),
        }
    }
}

/// Nombres de herramienta de Codex → los de Claude Code (misma clasificación y 1-shot).
fn canonical_tool(name: &str) -> &str {
    match name {
        "shell" | "shell_command" | "local_shell" | "exec_command" | "container.exec" => "Bash",
        "apply_patch" => "Edit",
        "write_file" | "create_file" => "Write",
        "read_file" | "view_image" => "Read",
        "grep_files" | "search" => "Grep",
        "list_dir" => "LS",
        "web_search" => "WebSearch",
        "fetch_url" | "web_fetch" => "WebFetch",
        "update_plan" => "TodoWrite",
        "spawn_agent" | "delegate" => "Agent",
        "request_user_input" => "AskUserQuestion",
        other => other,
    }
}

/// Objetivo legible de una llamada: comando de shell o archivo del parche.
fn tool_target(name: &str, args: &Value) -> Option<String> {
    let s = match name {
        "Bash" => match &args["command"] {
            Value::Array(parts) => {
                let parts: Vec<&str> = parts.iter().filter_map(|p| p.as_str()).collect();
                // `bash -lc "<cmd>"`: el comando real es el último argumento.
                if parts.len() >= 3 && parts[1].starts_with('-') && parts[1].contains('c') {
                    parts[2].to_string()
                } else {
                    parts.join(" ")
                }
            }
            Value::String(s) => s.clone(),
            _ => args["cmd"].as_str().unwrap_or("").to_string(),
        },
        "Edit" => {
            let patch = args["input"].as_str().or(args["patch"].as_str()).unwrap_or("");
            patch
                .lines()
                .find_map(|l| l.strip_prefix("*** Update File: ").or(l.strip_prefix("*** Add File: ")).or(l.strip_prefix("*** Delete File: ")))
                .unwrap_or("")
                .to_string()
        }
        _ => ["path", "file_path", "pattern", "query", "url", "description"]
            .iter()
            .find_map(|k| args[*k].as_str())
            .unwrap_or("")
            .to_string(),
    };
    (!s.is_empty()).then(|| s.chars().take(500).collect())
}

/// `function_call_output.output` es texto o `{"content","success"}`; el texto antiguo de shell
/// es un JSON con `metadata.exit_code`.
fn output_is_error(output: &Value) -> bool {
    if let Some(ok) = output["success"].as_bool() {
        return !ok;
    }
    if let Some(text) = output.as_str() {
        if let Ok(v) = serde_json::from_str::<Value>(text) {
            if let Some(code) = v["metadata"]["exit_code"].as_i64() {
                return code != 0;
            }
        }
    }
    false
}

impl Provider for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn name(&self) -> &'static str {
        "Codex CLI"
    }

    fn log_roots(&self) -> Vec<PathBuf> {
        if let Some(r) = &self.roots {
            return r.clone();
        }
        Self::home().map(|h| vec![h.join("sessions"), h.join("archived_sessions")]).unwrap_or_default()
    }

    fn matches(&self, path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "jsonl")
            && path.file_name().and_then(|f| f.to_str()).is_some_and(|f| f.starts_with("rollout-"))
            && self.log_roots().iter().any(|r| path.starts_with(r))
    }

    fn reset(&self, path: &Path) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).remove(path);
    }

    fn parse_line(&self, path: &Path, line: &str) -> Result<Vec<Record>> {
        let v: Value = serde_json::from_str(line)?;
        let Some(ts) = v["timestamp"].as_str().and_then(parse_ts) else {
            return Ok(vec![]);
        };
        let mut states = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let st = states.entry(path.to_path_buf()).or_default();
        let payload = &v["payload"];
        let mut out = Vec::new();

        match v["type"].as_str() {
            Some("session_meta") => {
                st.thread_id = payload["id"].as_str().or(payload["session_id"].as_str()).unwrap_or("").to_string();
                st.cwd = payload["cwd"].as_str().map(str::to_string);
                st.branch = payload["git"]["branch"].as_str().map(str::to_string);
                st.is_subagent = payload["parent_thread_id"].is_string()
                    || payload["thread_source"].as_str() == Some("subagent")
                    || payload["source"].get("sub_agent").is_some();
            }
            Some("turn_context") => {
                if let Some(m) = payload["model"].as_str() {
                    st.model = m.to_string();
                }
                if let Some(c) = payload["cwd"].as_str() {
                    st.cwd = Some(c.to_string());
                }
                let id = payload["turn_id"].as_str().map(str::to_string).unwrap_or_else(|| format!("{}:{ts}", st.thread_id));
                st.turn_id = Some(id.clone());
                if !st.is_subagent && !st.thread_id.is_empty() {
                    out.push(Record::Turn(TurnRec {
                        id,
                        session_id: st.thread_id.clone(),
                        ts,
                        intent: st.pending_prompt.take().as_deref().and_then(prompt_intent),
                    }));
                }
            }
            Some("token_usage_record") => {
                st.has_usage_records = true;
                push_call(&mut out, st, payload["response_id"].as_str().unwrap_or("").to_string(), &payload["usage"], ts);
            }
            Some("event_msg") => match payload["type"].as_str() {
                Some("user_message") => {
                    let text = payload["message"].as_str().unwrap_or("").to_string();
                    match &st.turn_id {
                        // El prompt llega tras su turn_context: completa la intención del turno.
                        Some(turn) if !st.is_subagent => out.push(Record::Turn(TurnRec {
                            id: turn.clone(),
                            session_id: st.thread_id.clone(),
                            ts,
                            intent: prompt_intent(&text),
                        })),
                        _ => st.pending_prompt = Some(text),
                    }
                }
                Some("token_count") if !st.has_usage_records => {
                    let usage = &payload["info"]["last_token_usage"];
                    if usage.is_object() {
                        let id = format!("tc:{}:{ts}", st.thread_id);
                        push_call(&mut out, st, id, usage, ts);
                    }
                }
                Some("turn_aborted") => out.push(Record::Event(EventRec {
                    session_id: st.thread_id.clone(),
                    ts,
                    kind: "interruption".into(),
                    payload_json: None,
                })),
                _ => {}
            },
            Some("compacted") => out.push(Record::Event(EventRec {
                session_id: st.thread_id.clone(),
                ts,
                kind: "compaction".into(),
                payload_json: None,
            })),
            Some("response_item") => match payload["type"].as_str() {
                Some("function_call") | Some("custom_tool_call") | Some("local_shell_call") => {
                    let raw = payload["name"].as_str().unwrap_or("local_shell");
                    let namespace = payload["namespace"].as_str().filter(|n| *n != "functions");
                    let tool = match namespace {
                        Some(ns) => format!("mcp__{ns}__{raw}"),
                        None => canonical_tool(raw).to_string(),
                    };
                    let args: Value = match payload["type"].as_str() {
                        Some("local_shell_call") => payload["action"].clone(),
                        Some("custom_tool_call") => serde_json::json!({ "input": payload["input"] }),
                        _ => payload["arguments"].as_str().and_then(|a| serde_json::from_str(a).ok()).unwrap_or(Value::Null),
                    };
                    let detail = (tool == "Agent").then(|| {
                        args["agent_type"].as_str().or(args["role"].as_str()).or(args["name"].as_str()).unwrap_or("general-purpose").to_string()
                    });
                    let Some(call_id) = payload["call_id"].as_str().or(payload["id"].as_str()) else {
                        return Ok(out);
                    };
                    out.push(Record::ToolUse(ToolUseRec {
                        call_id: call_id.to_string(),
                        message_id: st.turn_id.clone().unwrap_or_default(),
                        session_id: st.thread_id.clone(),
                        ts,
                        target: tool_target(&tool, &args),
                        tool,
                        detail,
                    }));
                }
                Some("function_call_output") | Some("custom_tool_call_output") => {
                    if let Some(call_id) = payload["call_id"].as_str() {
                        out.push(Record::ToolResult {
                            call_id: call_id.to_string(),
                            ts,
                            is_error: output_is_error(&payload["output"]),
                            agent_id: None,
                        });
                    }
                }
                _ => {}
            },
            _ => {}
        }

        if !out.is_empty() && !st.thread_id.is_empty() {
            out.insert(
                0,
                Record::Session(SessionRec {
                    id: st.thread_id.clone(),
                    cwd: st.cwd.clone(),
                    git_branch: st.branch.clone(),
                    ts,
                    is_subagent: st.is_subagent,
                }),
            );
        }
        Ok(out)
    }
}

fn push_call(out: &mut Vec<Record>, st: &FileState, message_id: String, usage: &Value, ts: i64) {
    if st.thread_id.is_empty() || message_id.is_empty() {
        return;
    }
    let n = |k: &str| usage[k].as_i64().unwrap_or(0);
    // `input_tokens` de Codex incluye los tokens leídos de caché.
    let cached = n("cached_input_tokens");
    out.push(Record::Call(CallRec {
        message_id,
        session_id: st.thread_id.clone(),
        ts,
        model: crate::pricing::normalize_model(&st.model),
        input_tokens: (n("input_tokens") - cached).max(0),
        output_tokens: n("output_tokens"),
        cache_read: cached,
        cache_write: n("cache_write_input_tokens"),
        cache_write_1h: 0,
        reasoning_tokens: n("reasoning_output_tokens"),
        activity: None,
        is_sidechain: st.is_subagent,
        agent_id: st.is_subagent.then(|| st.thread_id.clone()),
        cost_reported: None,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objetivo_de_shell_y_parche() {
        let args = serde_json::json!({ "command": ["bash", "-lc", "git status | head"], "workdir": "/p" });
        assert_eq!(tool_target("Bash", &args).as_deref(), Some("git status | head"));
        let args = serde_json::json!({ "command": ["ls", "-la"] });
        assert_eq!(tool_target("Bash", &args).as_deref(), Some("ls -la"));
        let patch = serde_json::json!({ "input": "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n-a\n+b\n*** End Patch" });
        assert_eq!(tool_target("Edit", &patch).as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn error_en_la_salida() {
        assert!(output_is_error(&serde_json::json!({ "content": "x", "success": false })));
        assert!(!output_is_error(&serde_json::json!({ "content": "x", "success": true })));
        assert!(output_is_error(&Value::String(r#"{"output":"boom","metadata":{"exit_code":1,"duration_seconds":0.1}}"#.into())));
        assert!(!output_is_error(&Value::String("todo bien".into())));
    }

    #[test]
    fn detecta_rollouts() {
        let p = Codex::with_roots(vec![PathBuf::from("/h/.codex/sessions")]);
        assert!(p.matches(Path::new("/h/.codex/sessions/2026/09/25/rollout-2026-09-25T10-00-00-abc.jsonl")));
        assert!(!p.matches(Path::new("/h/.codex/sessions/2026/09/25/otro.jsonl")));
        assert!(!p.matches(Path::new("/h/.codex/history.jsonl")));
    }
}
