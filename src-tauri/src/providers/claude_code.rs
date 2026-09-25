//! Sesiones de Claude Code: `~/.claude/projects/<proyecto>/<sesión>.jsonl`.

use super::{classify_tool, parse_ts, CallRec, EventRec, Provider, Record, SessionRec, ToolUseRec};
use anyhow::Result;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct ClaudeCode {
    /// Carpetas fijadas a mano (tests); si es `None` se resuelven del entorno.
    roots: Option<Vec<PathBuf>>,
}

impl ClaudeCode {
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self { roots: Some(roots) }
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

    fn matches(&self, path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "jsonl")
            && self.log_roots().iter().any(|r| path.starts_with(r))
    }

    fn parse_line(&self, _path: &Path, line: &str) -> Result<Vec<Record>> {
        let v: Value = serde_json::from_str(line)?;
        let Some(session_id) = v["sessionId"].as_str() else {
            return Ok(vec![]);
        };
        let Some(ts) = v["timestamp"].as_str().and_then(parse_ts) else {
            return Ok(vec![]);
        };
        let session_id = session_id.to_string();
        let mut out = vec![Record::Session(SessionRec {
            id: session_id.clone(),
            cwd: v["cwd"].as_str().map(str::to_string),
            git_branch: v["gitBranch"].as_str().filter(|b| !b.is_empty()).map(str::to_string),
            ts,
            is_subagent: false,
        })];

        match v["type"].as_str() {
            Some("assistant") => parse_assistant(&v, &session_id, ts, &mut out),
            Some("user") => parse_user(&v, &session_id, ts, &mut out),
            Some("system") => {
                if v["subtype"].as_str() == Some("compact_boundary") {
                    out.push(Record::Event(EventRec {
                        session_id,
                        ts,
                        kind: "compaction".into(),
                        payload_json: v.get("compactMetadata").map(|m| m.to_string()),
                    }));
                }
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
            let target = tool_target(&b["input"]);
            activity = Some(classify_tool(&tool, target.as_deref()).to_string());
            if let Some(call_id) = b["id"].as_str() {
                out.push(Record::ToolUse(ToolUseRec {
                    call_id: call_id.to_string(),
                    message_id: message_id.clone(),
                    session_id: session_id.to_string(),
                    ts,
                    tool,
                    target,
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
            }),
        );
    }
}

fn parse_user(v: &Value, session_id: &str, ts: i64, out: &mut Vec<Record>) {
    let content = &v["message"]["content"];
    if let Some(blocks) = content.as_array() {
        for b in blocks {
            match b["type"].as_str() {
                Some("tool_result") => {
                    if let Some(id) = b["tool_use_id"].as_str() {
                        out.push(Record::ToolResult {
                            call_id: id.to_string(),
                            ts,
                            is_error: b["is_error"].as_bool().unwrap_or(false),
                        });
                    }
                }
                Some("text") if is_interruption(b["text"].as_str()) => {
                    out.push(interruption(session_id, ts));
                }
                _ => {}
            }
        }
    } else if is_interruption(content.as_str()) {
        out.push(interruption(session_id, ts));
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
    ["file_path", "command", "path", "notebook_path", "url", "pattern", "query", "description"]
        .iter()
        .find_map(|k| input[*k].as_str())
        .map(|s| s.chars().take(500).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Vec<Record> {
        ClaudeCode::with_roots(vec![]).parse_line(Path::new("x.jsonl"), line).unwrap()
    }

    #[test]
    fn respuesta_con_uso_genera_llamada() {
        let line = r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-09-25T09:57:51.467Z","cwd":"/h/p/AgentBoard","gitBranch":"main",
          "message":{"model":"claude-opus-5-5","id":"msg_1","content":[{"type":"text","text":"hola"}],
          "usage":{"input_tokens":2,"output_tokens":138,"cache_read_input_tokens":25232,"cache_creation_input_tokens":13892,
                   "cache_creation":{"ephemeral_1h_input_tokens":13892},"output_tokens_details":{"thinking_tokens":64}}}}"#;
        let recs = parse(line);
        let Record::Call(c) = &recs[1] else { panic!("esperaba Call: {recs:?}") };
        assert_eq!((c.input_tokens, c.output_tokens, c.cache_read, c.cache_write), (2, 138, 25232, 13892));
        assert_eq!(c.cache_write_1h, 13892);
        assert_eq!(c.reasoning_tokens, 64);
        assert_eq!(c.model, "claude-opus-5-5");
        let Record::Session(s) = &recs[0] else { panic!() };
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
        assert!(parse(u).contains(&Record::ToolResult { call_id: "toolu_1".into(), ts: parse_ts("2026-09-25T10:00:05Z").unwrap(), is_error: true }));
    }

    #[test]
    fn lineas_sin_uso_o_desconocidas_se_ignoran() {
        assert!(parse(r#"{"type":"file-history-snapshot","messageId":"x"}"#).is_empty());
        let synth = r#"{"type":"assistant","sessionId":"s","timestamp":"2026-09-25T10:00:00Z","message":{"model":"<synthetic>","id":"m","content":[],"usage":{"input_tokens":0}}}"#;
        assert!(!parse(synth).iter().any(|r| matches!(r, Record::Call(_))));
        assert!(ClaudeCode::default().parse_line(Path::new("x"), "{no json").is_err());
    }

    #[test]
    fn compactacion_es_evento() {
        let l = r#"{"type":"system","subtype":"compact_boundary","sessionId":"s","timestamp":"2026-09-25T10:00:00Z","compactMetadata":{"trigger":"auto","preTokens":150000}}"#;
        assert!(parse(l).iter().any(|r| matches!(r, Record::Event(e) if e.kind == "compaction")));
    }

    #[test]
    fn respeta_claude_config_dir() {
        let p = ClaudeCode::with_roots(vec![PathBuf::from("/opt/claude/projects")]);
        assert!(p.matches(Path::new("/opt/claude/projects/a/b.jsonl")));
        assert!(!p.matches(Path::new("/otra/b.jsonl")));
    }
}
