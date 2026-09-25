//! Sesiones de Claude Code: `~/.claude/projects/<proyecto>/<sesión>.jsonl`.

use super::{
    classify_tool, parse_ts, prompt_intent, CallRec, EventRec, Provider, Record, SessionRec,
    ToolUseRec, TurnRec,
};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
pub struct ClaudeCode {
    /// Carpetas fijadas a mano (tests); si es `None` se resuelven del entorno.
    roots: Option<Vec<PathBuf>>,
    /// Primer `cwd` visto por archivo: el `cwd` de cada línea cambia con los `cd` del shell,
    /// pero el proyecto es la carpeta donde arrancó la sesión.
    first_cwd: Mutex<HashMap<PathBuf, String>>,
}

impl ClaudeCode {
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self {
            roots: Some(roots),
            first_cwd: Mutex::default(),
        }
    }
}

impl Provider for ClaudeCode {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn log_roots(&self) -> Vec<PathBuf> {
        if let Some(r) = &self.roots {
            return r.clone();
        }
        match std::env::var_os("CLAUDE_CONFIG_DIR") {
            Some(dir) if !dir.is_empty() => vec![PathBuf::from(dir).join("projects")],
            _ => dirs::home_dir()
                .map(|h| vec![h.join(".claude").join("projects")])
                .unwrap_or_default(),
        }
    }

    fn installed(&self) -> bool {
        self.log_roots()
            .iter()
            .any(|r| r.is_dir() || r.parent().is_some_and(|p| p.is_dir()))
            || super::on_path("claude")
    }

    fn matches(&self, path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "jsonl")
            && self.log_roots().iter().any(|r| path.starts_with(r))
    }

    fn reset(&self, path: &Path) {
        self.first_cwd
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(path);
    }

    fn parse_line(&self, path: &Path, line: &str) -> Result<Vec<Record>> {
        let v: Value = serde_json::from_str(line)?;
        let Some(session_id) = v["sessionId"].as_str() else {
            return Ok(vec![]);
        };
        let Some(ts) = v["timestamp"].as_str().and_then(parse_ts) else {
            return Ok(vec![]);
        };
        let session_id = session_id.to_string();
        let cwd = {
            let mut map = self.first_cwd.lock().unwrap_or_else(|e| e.into_inner());
            match (map.get(path), v["cwd"].as_str()) {
                (Some(first), _) => Some(first.clone()),
                (None, Some(c)) => {
                    map.insert(path.to_path_buf(), c.to_string());
                    Some(c.to_string())
                }
                (None, None) => None,
            }
        };
        let mut out = vec![Record::Session(SessionRec {
            id: session_id.clone(),
            cwd,
            git_branch: v["gitBranch"]
                .as_str()
                .filter(|b| !b.is_empty())
                .map(str::to_string),
            ts,
            is_subagent: false,
        })];

        match v["type"].as_str() {
            Some("assistant") => parse_assistant(&v, &session_id, ts, &mut out),
            Some("user") => parse_user(&v, &session_id, ts, &mut out),
            Some("system") if v["subtype"].as_str() == Some("compact_boundary") => {
                out.push(Record::Event(EventRec {
                    session_id,
                    ts,
                    kind: "compaction".into(),
                    payload_json: v.get("compactMetadata").map(|m| m.to_string()),
                }));
            }
            _ => {}
        }
        Ok(out)
    }
}

fn parse_assistant(v: &Value, session_id: &str, ts: i64, out: &mut Vec<Record>) {
    let msg = &v["message"];
    let model = msg["model"].as_str().unwrap_or("");
    let message_id = msg["id"]
        .as_str()
        .or_else(|| v["uuid"].as_str())
        .unwrap_or_default()
        .to_string();
    if message_id.is_empty() {
        return;
    }

    let mut activity = None;
    if let Some(blocks) = msg["content"].as_array() {
        for b in blocks.iter().filter(|b| b["type"] == "tool_use") {
            let tool = b["name"].as_str().unwrap_or("?").to_string();
            let input = &b["input"];
            let target = tool_target(input);
            let detail = match tool.as_str() {
                "Skill" => input["skill"].as_str().map(str::to_string),
                "Agent" | "Task" => Some(
                    input["subagent_type"]
                        .as_str()
                        .unwrap_or("general-purpose")
                        .to_string(),
                ),
                _ => None,
            };
            activity = Some(classify_tool(&tool, target.as_deref()).to_string());
            if let Some(call_id) = b["id"].as_str() {
                out.push(Record::ToolUse(ToolUseRec {
                    call_id: call_id.to_string(),
                    message_id: message_id.clone(),
                    session_id: session_id.to_string(),
                    ts,
                    tool,
                    target,
                    detail,
                }));
            }
        }
    }

    // Mensajes sintéticos (errores locales) no tienen coste real.
    let usage = &msg["usage"];
    if usage.is_object() && !model.is_empty() && model != "<synthetic>" {
        let n = |x: &Value| x.as_i64().unwrap_or(0);
        out.insert(
            1,
            Record::Call(CallRec {
                message_id,
                session_id: session_id.to_string(),
                ts,
                model: crate::pricing::normalize_model(model),
                input_tokens: n(&usage["input_tokens"]),
                output_tokens: n(&usage["output_tokens"]),
                cache_read: n(&usage["cache_read_input_tokens"]),
                cache_write: n(&usage["cache_creation_input_tokens"]),
                cache_write_1h: n(&usage["cache_creation"]["ephemeral_1h_input_tokens"]),
                reasoning_tokens: n(&usage["output_tokens_details"]["thinking_tokens"]),
                activity,
                is_sidechain: v["isSidechain"].as_bool().unwrap_or(false),
                agent_id: v["agentId"].as_str().map(str::to_string),
                cost_reported: None,
            }),
        );
    }
}

fn parse_user(v: &Value, session_id: &str, ts: i64, out: &mut Vec<Record>) {
    let content = &v["message"]["content"];
    let sidechain = v["isSidechain"].as_bool().unwrap_or(false);
    // El texto del prompt humano: se usa para la intención y se descarta.
    let mut prompt: Option<String> = None;
    if let Some(blocks) = content.as_array() {
        for b in blocks {
            match b["type"].as_str() {
                Some("tool_result") => {
                    if let Some(id) = b["tool_use_id"].as_str() {
                        out.push(Record::ToolResult {
                            call_id: id.to_string(),
                            ts,
                            is_error: b["is_error"].as_bool().unwrap_or(false),
                            agent_id: v["toolUseResult"]["agentId"].as_str().map(str::to_string),
                        });
                    }
                }
                Some("text") if is_interruption(b["text"].as_str()) => {
                    out.push(interruption(session_id, ts));
                }
                Some("text") => {
                    prompt
                        .get_or_insert_with(String::new)
                        .push_str(b["text"].as_str().unwrap_or(""));
                }
                _ => {}
            }
        }
    } else if is_interruption(content.as_str()) {
        out.push(interruption(session_id, ts));
    } else if let Some(text) = content.as_str() {
        prompt = Some(text.to_string());
    }

    // Las líneas de un turno comparten promptId; las de subagentes no abren turno propio.
    if let (Some(id), false) = (v["promptId"].as_str(), sidechain) {
        let is_human = !v["isMeta"].as_bool().unwrap_or(false);
        out.push(Record::Turn(TurnRec {
            id: id.to_string(),
            session_id: session_id.to_string(),
            ts,
            intent: prompt
                .filter(|_| is_human)
                .as_deref()
                .and_then(prompt_intent),
        }));
    }
}

fn is_interruption(text: Option<&str>) -> bool {
    text.is_some_and(|t| t.starts_with("[Request interrupted by user"))
}

fn interruption(session_id: &str, ts: i64) -> Record {
    Record::Event(EventRec {
        session_id: session_id.to_string(),
        ts,
        kind: "interruption".into(),
        payload_json: None,
    })
}

/// Archivo o comando sobre el que actúa una herramienta.
fn tool_target(input: &Value) -> Option<String> {
    [
        "file_path",
        "command",
        "path",
        "notebook_path",
        "url",
        "pattern",
        "query",
        "description",
    ]
    .iter()
    .find_map(|k| input[*k].as_str())
    .map(|s| s.chars().take(500).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Vec<Record> {
        ClaudeCode::with_roots(vec![])
            .parse_line(Path::new("x.jsonl"), line)
            .unwrap()
    }

    #[test]
    fn respuesta_con_uso_genera_llamada() {
        let line = r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-09-25T09:57:51.467Z","cwd":"/h/p/AgentBoard","gitBranch":"main",
          "message":{"model":"claude-opus-5-5","id":"msg_1","content":[{"type":"text","text":"hola"}],
          "usage":{"input_tokens":2,"output_tokens":138,"cache_read_input_tokens":25232,"cache_creation_input_tokens":13892,
                   "cache_creation":{"ephemeral_1h_input_tokens":13892},"output_tokens_details":{"thinking_tokens":64}}}}"#;
        let recs = parse(line);
        let Record::Call(c) = &recs[1] else {
            panic!("esperaba Call: {recs:?}")
        };
        assert_eq!(
            (c.input_tokens, c.output_tokens, c.cache_read, c.cache_write),
            (2, 138, 25232, 13892)
        );
        assert_eq!(c.cache_write_1h, 13892);
        assert_eq!(c.reasoning_tokens, 64);
        assert_eq!(c.model, "claude-opus-5-5");
        let Record::Session(s) = &recs[0] else {
            panic!()
        };
        assert_eq!(s.git_branch.as_deref(), Some("main"));
    }

    #[test]
    fn tool_use_y_resultado_con_error() {
        let a = r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-09-25T10:00:00Z","cwd":"/p",
          "message":{"model":"claude-opus-5-5","id":"msg_2","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"npm install"}}],
          "usage":{"input_tokens":1,"output_tokens":1}}}"#;
        let recs = parse(a);
        assert!(recs.iter().any(|r| matches!(r, Record::ToolUse(t) if t.tool == "Bash" && t.target.as_deref() == Some("npm install"))));
        let u = r#"{"type":"user","sessionId":"s1","timestamp":"2026-09-25T10:00:05Z","cwd":"/p",
          "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","is_error":true,"content":"x"}]}}"#;
        assert!(parse(u).contains(&Record::ToolResult {
            call_id: "toolu_1".into(),
            ts: parse_ts("2026-09-25T10:00:05Z").unwrap(),
            is_error: true,
            agent_id: None
        }));
    }

    #[test]
    fn lineas_sin_uso_o_desconocidas_se_ignoran() {
        assert!(parse(r#"{"type":"file-history-snapshot","messageId":"x"}"#).is_empty());
        let synth = r#"{"type":"assistant","sessionId":"s","timestamp":"2026-09-25T10:00:00Z","message":{"model":"<synthetic>","id":"m","content":[],"usage":{"input_tokens":0}}}"#;
        assert!(!parse(synth).iter().any(|r| matches!(r, Record::Call(_))));
        assert!(ClaudeCode::default()
            .parse_line(Path::new("x"), "{no json")
            .is_err());
    }

    #[test]
    fn prompt_abre_turno_con_intencion_sin_texto() {
        let u = r#"{"type":"user","sessionId":"s1","promptId":"p1","timestamp":"2026-09-25T10:00:00Z","cwd":"/p",
          "message":{"role":"user","content":"Arregla el test que falla"}}"#;
        let recs = parse(u);
        let turn = recs
            .iter()
            .find_map(|r| {
                if let Record::Turn(t) = r {
                    Some(t)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!((turn.id.as_str(), turn.intent), ("p1", Some("debug")));
        assert!(
            !format!("{recs:?}").contains("Arregla"),
            "el texto no sale del parser"
        );
        let side = u.replace(r#""promptId""#, r#""isSidechain":true,"promptId""#);
        assert!(!parse(&side).iter().any(|r| matches!(r, Record::Turn(_))));
    }

    #[test]
    fn skills_y_subagentes() {
        let a = r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-09-25T10:00:00Z",
          "message":{"model":"claude-opus-5-5","id":"m","content":[
            {"type":"tool_use","id":"t1","name":"Skill","input":{"skill":"dataviz"}},
            {"type":"tool_use","id":"t2","name":"Agent","input":{"description":"x","prompt":"y","subagent_type":"Explore"}}],
          "usage":{"input_tokens":1,"output_tokens":1}}}"#;
        let details: Vec<_> = parse(a)
            .into_iter()
            .filter_map(|r| {
                if let Record::ToolUse(t) = r {
                    t.detail
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(details, vec!["dataviz".to_string(), "Explore".to_string()]);

        let res = r#"{"type":"user","sessionId":"s1","timestamp":"2026-09-25T10:01:00Z","toolUseResult":{"agentId":"a1","status":"completed"},
          "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","content":"ok"}]}}"#;
        assert!(parse(res)
            .iter()
            .any(|r| matches!(r, Record::ToolResult { agent_id: Some(a), .. } if a == "a1")));

        let side = r#"{"type":"assistant","isSidechain":true,"agentId":"a1","sessionId":"s1","timestamp":"2026-09-25T10:00:30Z",
          "message":{"model":"claude-haiku-4-5","id":"ms","content":[],"usage":{"input_tokens":1,"output_tokens":1}}}"#;
        assert!(parse(side).iter().any(|r| matches!(r, Record::Call(c) if c.is_sidechain && c.agent_id.as_deref() == Some("a1"))));
    }

    #[test]
    fn compactacion_es_evento() {
        let l = r#"{"type":"system","subtype":"compact_boundary","sessionId":"s","timestamp":"2026-09-25T10:00:00Z","compactMetadata":{"trigger":"auto","preTokens":150000}}"#;
        assert!(parse(l)
            .iter()
            .any(|r| matches!(r, Record::Event(e) if e.kind == "compaction")));
    }

    #[test]
    fn el_proyecto_es_la_carpeta_de_arranque() {
        let p = ClaudeCode::with_roots(vec![]);
        let path = Path::new("s.jsonl");
        let a = r#"{"type":"user","sessionId":"s1","timestamp":"2026-09-25T10:00:00Z","cwd":"/h/repo","message":{"role":"user","content":"hola"}}"#;
        let b = r#"{"type":"user","sessionId":"s1","timestamp":"2026-09-25T10:01:00Z","cwd":"/h/repo/src","message":{"role":"user","content":"hola"}}"#;
        p.parse_line(path, a).unwrap();
        let recs = p.parse_line(path, b).unwrap();
        let Record::Session(s) = &recs[0] else {
            panic!()
        };
        assert_eq!(
            s.cwd.as_deref(),
            Some("/h/repo"),
            "el cd del shell no crea otro proyecto"
        );
        p.reset(path);
        let recs = p.parse_line(path, b).unwrap();
        let Record::Session(s) = &recs[0] else {
            panic!()
        };
        assert_eq!(s.cwd.as_deref(), Some("/h/repo/src"));
    }

    #[test]
    fn respeta_claude_config_dir() {
        let p = ClaudeCode::with_roots(vec![PathBuf::from("/opt/claude/projects")]);
        assert!(p.matches(Path::new("/opt/claude/projects/a/b.jsonl")));
        assert!(!p.matches(Path::new("/otra/b.jsonl")));
    }
}
