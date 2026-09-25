//! GitHub Copilot CLI: `~/.copilot/session-state/<sesión>/events.jsonl` (`COPILOT_HOME` lo cambia).
//! Cada línea es `{"id","timestamp","type","data"}` con tipos `session.start`, `user.message`,
//! `assistant.turn_start`, `assistant.message`, `tool.execution_start|complete`, `session.shutdown`…
//!
//! Solo `assistant.message` trae tokens de salida por respuesta; los de entrada y caché llegan
//! agregados por modelo en `session.shutdown`, así que al cerrar la sesión se reparten entre
//! las respuestas de ese modelo desde el cierre anterior (una sesión reanudada cierra varias veces).

use super::{parse_ts, prompt_intent, CallRec, EventRec, Provider, Record, SessionRec, ToolUseRec, TurnRec};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
pub struct Copilot {
    roots: Option<Vec<PathBuf>>,
    state: Mutex<HashMap<PathBuf, FileState>>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Usage {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
}

impl Usage {
    fn from_json(v: &Value) -> Self {
        let n = |k: &str| v[k].as_i64().unwrap_or(0);
        Self { input: n("inputTokens"), output: n("outputTokens"), cache_read: n("cacheReadTokens"), cache_write: n("cacheWriteTokens"), reasoning: n("reasoningTokens") }
    }
    fn delta(self, prev: Self) -> Self {
        let d = |a: i64, b: i64| (a - b).max(0);
        Self {
            input: d(self.input, prev.input),
            output: d(self.output, prev.output),
            cache_read: d(self.cache_read, prev.cache_read),
            cache_write: d(self.cache_write, prev.cache_write),
            reasoning: d(self.reasoning, prev.reasoning),
        }
    }
}

#[derive(Default)]
struct FileState {
    session_id: String,
    cwd: Option<String>,
    branch: Option<String>,
    model: String,
    turn_id: Option<String>,
    turns: u32,
    pending_prompt: Option<String>,
    /// Respuestas por modelo desde el último `session.shutdown`: (message_id, ts, tokens de salida).
    pending_calls: HashMap<String, Vec<(String, i64, i64)>>,
    /// Último total acumulado por modelo, para calcular el delta del siguiente cierre.
    last_usage: HashMap<String, Usage>,
}

impl Copilot {
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self { roots: Some(roots), state: Mutex::default() }
    }

    fn home() -> Option<PathBuf> {
        match std::env::var_os("COPILOT_HOME") {
            Some(h) if !h.is_empty() => Some(PathBuf::from(h)),
            _ => dirs::home_dir().map(|h| h.join(".copilot")),
        }
    }
}

/// Nombres de herramienta de Copilot → los de Claude Code.
fn canonical_tool(name: &str) -> &str {
    match name {
        "bash" | "shell" | "powershell" => "Bash",
        "view" | "read" | "read_file" => "Read",
        "create" | "create_file" | "write" => "Write",
        "edit" | "str_replace" | "str_replace_editor" | "apply_patch" => "Edit",
        "grep" | "grep_search" => "Grep",
        "glob" | "file_search" => "Glob",
        "web_fetch" | "fetch" => "WebFetch",
        "web_search" => "WebSearch",
        "task" | "subagent" | "delegate" => "Agent",
        "skill" => "Skill",
        "ask_user" | "request_user_input" => "AskUserQuestion",
        other => other,
    }
}

/// `timestamp` llega como epoch ms (número) o como fecha RFC 3339.
fn line_ts(v: &Value) -> Option<i64> {
    v["timestamp"].as_i64().or_else(|| v["timestamp"].as_str().and_then(parse_ts))
}

impl Provider for Copilot {
    fn id(&self) -> &'static str {
        "copilot"
    }

    fn name(&self) -> &'static str {
        "GitHub Copilot CLI"
    }

    fn log_roots(&self) -> Vec<PathBuf> {
        if let Some(r) = &self.roots {
            return r.clone();
        }
        Self::home().map(|h| vec![h.join("session-state")]).unwrap_or_default()
    }

    fn matches(&self, path: &Path) -> bool {
        path.file_name().is_some_and(|f| f == "events.jsonl") && self.log_roots().iter().any(|r| path.starts_with(r))
    }

    fn reset(&self, path: &Path) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).remove(path);
    }

    fn parse_line(&self, path: &Path, line: &str) -> Result<Vec<Record>> {
        let v: Value = serde_json::from_str(line)?;
        let Some(ts) = line_ts(&v) else {
            return Ok(vec![]);
        };
        let mut states = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let st = states.entry(path.to_path_buf()).or_default();
        if st.session_id.is_empty() {
            // La carpeta de la sesión lleva su id; `session.start` lo confirma.
            st.session_id = path.parent().and_then(|p| p.file_name()).and_then(|f| f.to_str()).unwrap_or("").to_string();
        }
        let d = &v["data"];
        let mut out = Vec::new();

        match v["type"].as_str().unwrap_or("") {
            "session.start" | "session.resume" => {
                if let Some(id) = d["sessionId"].as_str() {
                    st.session_id = id.to_string();
                }
                if let Some(m) = d["selectedModel"].as_str().or(d["currentModel"].as_str()) {
                    st.model = m.to_string();
                }
                if let Some(c) = d["context"]["cwd"].as_str() {
                    st.cwd = Some(c.to_string());
                }
                if let Some(b) = d["context"]["branch"].as_str() {
                    st.branch = Some(b.to_string());
                }
            }
            "session.model_change" => {
                if let Some(m) = d["newModel"].as_str().or(d["selectedModel"].as_str()).or(d["model"].as_str()) {
                    st.model = m.to_string();
                }
            }
            "user.message" => st.pending_prompt = Some(d["content"].as_str().unwrap_or("").to_string()),
            "assistant.turn_start" => {
                st.turns += 1;
                let id = d["turnId"].as_str().map(str::to_string).unwrap_or_else(|| format!("{}:{}", st.session_id, st.turns));
                st.turn_id = Some(id.clone());
                out.push(Record::Turn(TurnRec { id, session_id: st.session_id.clone(), ts, intent: st.pending_prompt.take().as_deref().and_then(prompt_intent) }));
            }
            "assistant.message" => {
                if let Some(m) = d["model"].as_str() {
                    st.model = m.to_string();
                }
                if let Some(t) = d["turnId"].as_str() {
                    st.turn_id = Some(t.to_string());
                }
                let message_id = d["messageId"].as_str().or(v["id"].as_str()).unwrap_or("").to_string();
                let output = d["outputTokens"].as_i64().unwrap_or(0);
                if !message_id.is_empty() {
                    let model = crate::pricing::normalize_model(&st.model);
                    st.pending_calls.entry(model.clone()).or_default().push((message_id.clone(), ts, output));
                    out.push(call(&st.session_id, message_id, ts, model, Usage { output, ..Default::default() }));
                }
            }
            "tool.execution_start" | "subagent.started" | "skill.invoked" => {
                let kind = v["type"].as_str().unwrap_or("");
                let raw = d["toolName"].as_str().or(d["name"].as_str()).unwrap_or(if kind == "skill.invoked" { "skill" } else { "task" });
                let tool = if kind == "subagent.started" { "Agent".to_string() } else { canonical_tool(raw).to_string() };
                let args = &d["arguments"];
                let target = ["command", "path", "file_path", "pattern", "query", "url", "description"]
                    .iter()
                    .find_map(|k| args[*k].as_str())
                    .map(|s| s.chars().take(500).collect::<String>());
                let detail = match tool.as_str() {
                    "Agent" => Some(
                        ["agentName", "agentType", "subagentType", "name", "agent"].iter().find_map(|k| d[*k].as_str().or(args[*k].as_str())).unwrap_or("general-purpose").to_string(),
                    ),
                    "Skill" => ["skillName", "name", "skill"].iter().find_map(|k| d[*k].as_str().or(args[*k].as_str())).map(str::to_string),
                    _ => None,
                };
                let call_id = ["toolCallId", "subagentId", "agentId", "id"].iter().find_map(|k| d[*k].as_str()).or(v["id"].as_str()).unwrap_or("").to_string();
                if !call_id.is_empty() {
                    if let Some(t) = d["turnId"].as_str() {
                        st.turn_id = Some(t.to_string());
                    }
                    out.push(Record::ToolUse(ToolUseRec {
                        call_id,
                        message_id: st.turn_id.clone().unwrap_or_default(),
                        session_id: st.session_id.clone(),
                        ts,
                        tool,
                        target,
                        detail,
                    }));
                }
            }
            "tool.execution_complete" | "subagent.completed" => {
                if let Some(call_id) = ["toolCallId", "subagentId", "agentId", "id"].iter().find_map(|k| d[*k].as_str()) {
                    out.push(Record::ToolResult {
                        call_id: call_id.to_string(),
                        ts,
                        is_error: d["success"].as_bool() == Some(false) || d["error"].is_string(),
                        agent_id: None,
                    });
                }
            }
            "session.shutdown" => {
                // Reparte el delta de tokens de cada modelo entre sus respuestas pendientes.
                if let Some(metrics) = d["modelMetrics"].as_object() {
                    for (raw_model, m) in metrics {
                        let model = crate::pricing::normalize_model(raw_model);
                        let total = Usage::from_json(&m["usage"]);
                        let prev = st.last_usage.insert(model.clone(), total).unwrap_or_default();
                        let delta = total.delta(prev);
                        let calls = st.pending_calls.remove(&model).unwrap_or_default();
                        if calls.is_empty() {
                            // Sin respuestas registradas: una llamada agregada para no perder el coste.
                            if delta != Usage::default() {
                                out.push(call(&st.session_id, format!("{}:{model}:{ts}", st.session_id), ts, model, delta));
                            }
                            continue;
                        }
                        let n = calls.len() as i64;
                        let out_sum: i64 = calls.iter().map(|c| c.2).sum();
                        for (i, (id, cts, output)) in calls.iter().enumerate() {
                            let share = |v: i64| v / n + if (i as i64) < v % n { 1 } else { 0 };
                            // Si el total de salida supera lo ya contado, el resto va al último.
                            let extra_out = if i + 1 == calls.len() { (delta.output - out_sum).max(0) } else { 0 };
                            out.push(call(
                                &st.session_id,
                                id.clone(),
                                *cts,
                                model.clone(),
                                Usage { input: share(delta.input), output: output + extra_out, cache_read: share(delta.cache_read), cache_write: share(delta.cache_write), reasoning: share(delta.reasoning) },
                            ));
                        }
                    }
                }
            }
            "session.compaction_complete" => out.push(Record::Event(EventRec { session_id: st.session_id.clone(), ts, kind: "compaction".into(), payload_json: None })),
            "abort" => out.push(Record::Event(EventRec { session_id: st.session_id.clone(), ts, kind: "interruption".into(), payload_json: None })),
            _ => {}
        }

        if !out.is_empty() && !st.session_id.is_empty() {
            out.insert(0, Record::Session(SessionRec { id: st.session_id.clone(), cwd: st.cwd.clone(), git_branch: st.branch.clone(), ts, is_subagent: false }));
        }
        Ok(out)
    }
}

fn call(session_id: &str, message_id: String, ts: i64, model: String, u: Usage) -> Record {
    Record::Call(CallRec {
        message_id,
        session_id: session_id.to_string(),
        ts,
        model,
        input_tokens: u.input,
        output_tokens: u.output,
        cache_read: u.cache_read,
        cache_write: u.cache_write,
        cache_write_1h: 0,
        reasoning_tokens: u.reasoning,
        activity: None,
        is_sidechain: false,
        agent_id: None,
        cost_reported: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecta_eventos() {
        let p = Copilot::with_roots(vec![PathBuf::from("/h/.copilot/session-state")]);
        assert!(p.matches(Path::new("/h/.copilot/session-state/abc/events.jsonl")));
        assert!(!p.matches(Path::new("/h/.copilot/session-state/abc/workspace.yaml")));
    }

    #[test]
    fn fecha_numerica_o_texto() {
        assert_eq!(line_ts(&serde_json::json!({ "timestamp": 1770000000100i64 })), Some(1_770_000_000_100));
        assert_eq!(line_ts(&serde_json::json!({ "timestamp": "2026-09-25T09:57:51.467Z" })), Some(1_790_330_271_467));
    }

    #[test]
    fn delta_de_uso() {
        let a = Usage { input: 100, output: 10, cache_read: 50, cache_write: 5, reasoning: 1 };
        let b = Usage { input: 250, output: 30, cache_read: 50, cache_write: 20, reasoning: 1 };
        assert_eq!(b.delta(a), Usage { input: 150, output: 20, cache_read: 0, cache_write: 15, reasoning: 0 });
    }
}
