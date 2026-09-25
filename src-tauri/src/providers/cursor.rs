//! Cursor CLI (`cursor-agent`): transcripciones en
//! `~/.cursor/projects/<carpeta-con-guiones>/agent-transcripts/<id>/<id>.jsonl`
//! (subagentes en `<id>/subagents/*.jsonl`). Cada línea es `{"role": "user"|"assistant",
//! "message": {"content": [{"type": "text"|"tool_use"|"tool-call", ...}]}}`.
//!
//! Cursor no apunta tokens ni modelo en la transcripción: los tokens se estiman por caracteres
//! (≈4 por token) y el modelo queda como `cursor-auto`, sin precio. Sirve para ver actividad,
//! herramientas y turnos; el coste real lo da la suscripción de Cursor.

use super::{prompt_intent, CallRec, Provider, Record, SessionRec, ToolUseRec, TurnRec};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
pub struct Cursor {
    roots: Option<Vec<PathBuf>>,
    state: Mutex<HashMap<PathBuf, FileState>>,
}

#[derive(Default)]
struct FileState {
    session_id: String,
    cwd: Option<String>,
    is_subagent: bool,
    /// Marca de tiempo base (mtime del archivo) y contador de líneas para ordenar sin fechas.
    base_ts: i64,
    lines: i64,
    turns: u32,
    turn_id: Option<String>,
}

impl Cursor {
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self {
            roots: Some(roots),
            state: Mutex::default(),
        }
    }

    fn home() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".cursor"))
    }
}

/// `~/.cursor/projects/-home-dev-Proyectos-demo` → `/home/dev/Proyectos/demo` (aproximado: los
/// guiones del nombre original no se distinguen de los separadores).
fn cwd_from_project_dir(name: &str) -> Option<String> {
    if !name.starts_with('-') {
        return None;
    }
    Some(name.replace('-', "/"))
}

/// Texto del usuario dentro de `<user_query>…</user_query>` (el resto es contexto inyectado).
fn user_query(text: &str) -> Option<String> {
    let start = text.find("<user_query>")? + "<user_query>".len();
    let end = text[start..]
        .find("</user_query>")
        .map(|e| start + e)
        .unwrap_or(text.len());
    let q = text[start..end].trim();
    (!q.is_empty()).then(|| q.to_string())
}

/// `<timestamp>Thursday, Sep 3, 2026, 6:52 AM (UTC-7)</timestamp>` → epoch ms.
fn prompt_timestamp(text: &str) -> Option<i64> {
    let start = text.find("<timestamp>")? + "<timestamp>".len();
    let end = text[start..].find("</timestamp>")? + start;
    let raw = &text[start..end];
    // "Thursday, Sep 3, 2026, 6:52 AM (UTC-7)"
    let mut parts = raw.split(", ");
    parts.next()?; // día de la semana
    let month_day = parts.next()?; // "Sep 3"
    let year: i32 = parts.next()?.trim().parse().ok()?;
    let time_zone = parts.next()?; // "6:52 AM (UTC-7)"
    let (mon, day) = month_day.split_once(' ')?;
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| *m == mon)? as u32
        + 1;
    let day: u32 = day.trim().parse().ok()?;
    let (clock, rest) = time_zone.split_once(' ')?;
    let (h, m) = clock.split_once(':')?;
    let (mut hour, minute): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    let pm = rest.starts_with("PM");
    if pm && hour < 12 {
        hour += 12;
    }
    if !pm && hour == 12 {
        hour = 0;
    }
    let offset_min: i64 = rest
        .split("(UTC")
        .nth(1)
        .and_then(|z| {
            z.trim_end_matches(')')
                .split_once(':')
                .map(|(h, m)| (h.to_string(), m.to_string()))
                .or_else(|| Some((z.trim_end_matches(')').to_string(), "0".into())))
        })
        .and_then(|(h, m)| {
            let h: i64 = h.parse().ok()?;
            let m: i64 = m.parse().ok()?;
            Some(h * 60 + if h < 0 { -m } else { m })
        })
        .unwrap_or(0);
    let naive = chrono::NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, 0)?;
    Some(naive.and_utc().timestamp_millis() - offset_min * 60_000)
}

fn canonical_tool(name: &str) -> &str {
    match name {
        "shell" | "bash" | "run_terminal_cmd" | "run_terminal_command" => "Bash",
        "read" | "read_file" | "view" => "Read",
        "write" | "write_file" | "create_file" => "Write",
        "edit" | "edit_file" | "search_replace" | "str_replace" | "apply_patch" => "Edit",
        "grep" | "grep_search" | "ripgrep" => "Grep",
        "glob" | "glob_file_search" | "file_search" | "list_dir" => "Glob",
        "web_fetch" | "fetch" => "WebFetch",
        "web_search" => "WebSearch",
        "task" | "subagent" | "delegate" => "Agent",
        "todo_write" | "update_todos" => "TodoWrite",
        other => other,
    }
}

const CHARS_PER_TOKEN: usize = 4;

impl Provider for Cursor {
    fn id(&self) -> &'static str {
        "cursor"
    }

    fn name(&self) -> &'static str {
        "Cursor CLI"
    }

    fn log_roots(&self) -> Vec<PathBuf> {
        if let Some(r) = &self.roots {
            return r.clone();
        }
        Self::home()
            .map(|h| vec![h.join("projects")])
            .unwrap_or_default()
    }

    fn installed(&self) -> bool {
        Self::home().is_some_and(|h| {
            h.join("projects").is_dir()
                || h.join("chats").is_dir()
                || h.join("cli-config.json").exists()
        }) || super::on_path("cursor-agent")
    }

    fn matches(&self, path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "jsonl")
            && path
                .components()
                .any(|c| c.as_os_str() == "agent-transcripts")
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
        if st.session_id.is_empty() {
            st.session_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            st.is_subagent = path
                .parent()
                .and_then(|p| p.file_name())
                .is_some_and(|n| n == "subagents");
            // …/projects/<carpeta>/agent-transcripts/<id>/<id>.jsonl
            let mut anc = path.ancestors();
            let project_dir = anc.find(|a| {
                a.parent()
                    .and_then(|p| p.file_name())
                    .is_some_and(|n| n == "projects")
            });
            st.cwd = project_dir
                .and_then(|d| d.file_name())
                .and_then(|n| n.to_str())
                .and_then(cwd_from_project_dir);
            st.base_ts = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
        }
        st.lines += 1;
        // Sin fechas por línea: se ordenan a partir del mtime, un segundo por línea.
        let mut ts = st.base_ts + st.lines * 1000;
        let mut out = Vec::new();
        let blocks: Vec<Value> = match &v["message"]["content"] {
            Value::Array(a) => a.clone(),
            Value::String(s) => vec![serde_json::json!({ "type": "text", "text": s })],
            _ => vec![],
        };

        match v["role"].as_str() {
            Some("user") if !st.is_subagent => {
                let text: String = blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                let Some(query) = user_query(&text) else {
                    return Ok(vec![]);
                }; // contexto inyectado, no un prompt
                if let Some(t) = prompt_timestamp(&text) {
                    st.base_ts = t - st.lines * 1000;
                    ts = t;
                }
                st.turns += 1;
                let id = format!("{}:{}", st.session_id, st.turns);
                st.turn_id = Some(id.clone());
                out.push(Record::Turn(TurnRec {
                    id,
                    session_id: st.session_id.clone(),
                    ts,
                    intent: prompt_intent(&query),
                }));
            }
            Some("assistant") => {
                let mut chars = 0usize;
                let message_id = format!("{}:{}", st.session_id, st.lines);
                for b in &blocks {
                    match b["type"].as_str() {
                        Some("text") | Some("reasoning") => {
                            chars += b["text"].as_str().map(|t| t.len()).unwrap_or(0)
                        }
                        Some("tool_use") | Some("tool-call") => {
                            let raw = b["name"].as_str().or(b["toolName"].as_str()).unwrap_or("?");
                            let tool = canonical_tool(raw).to_string();
                            let args = if b["input"].is_object() {
                                &b["input"]
                            } else {
                                &b["args"]
                            };
                            chars += args.to_string().len();
                            let target = [
                                "command",
                                "path",
                                "file_path",
                                "target_file",
                                "relative_workspace_path",
                                "pattern",
                                "query",
                                "url",
                            ]
                            .iter()
                            .find_map(|k| args[*k].as_str())
                            .map(|s| s.chars().take(500).collect::<String>());
                            let call_id = b["id"]
                                .as_str()
                                .or(b["toolCallId"].as_str())
                                .map(str::to_string)
                                .unwrap_or_else(|| format!("{message_id}:{}", out.len()));
                            out.push(Record::ToolUse(ToolUseRec {
                                call_id,
                                message_id: message_id.clone(),
                                session_id: st.session_id.clone(),
                                ts,
                                target,
                                detail: (tool == "Agent").then(|| {
                                    args["subagent_type"]
                                        .as_str()
                                        .unwrap_or("general-purpose")
                                        .to_string()
                                }),
                                tool,
                            }));
                        }
                        _ => {}
                    }
                }
                if chars > 0 || !out.is_empty() {
                    out.push(Record::Call(CallRec {
                        message_id,
                        session_id: st.session_id.clone(),
                        ts,
                        model: "cursor-auto".into(),
                        input_tokens: 0,
                        output_tokens: (chars / CHARS_PER_TOKEN) as i64,
                        cache_read: 0,
                        cache_write: 0,
                        cache_write_1h: 0,
                        reasoning_tokens: 0,
                        activity: None,
                        is_sidechain: st.is_subagent,
                        agent_id: st.is_subagent.then(|| st.session_id.clone()),
                        cost_reported: None,
                    }));
                }
            }
            _ => {}
        }

        if !out.is_empty() {
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
    fn consulta_y_fecha_del_prompt() {
        let t = "<user_info>x</user_info><user_query>\nArregla el bug\n</user_query><timestamp>Thursday, Sep 3, 2026, 6:52 AM (UTC-7)</timestamp>";
        assert_eq!(user_query(t).as_deref(), Some("Arregla el bug"));
        assert_eq!(
            prompt_timestamp(t),
            Some(
                chrono::NaiveDate::from_ymd_opt(2026, 9, 3)
                    .unwrap()
                    .and_hms_opt(13, 52, 0)
                    .unwrap()
                    .and_utc()
                    .timestamp_millis()
            )
        );
        assert_eq!(user_query("<user_info>solo contexto</user_info>"), None);
    }

    #[test]
    fn carpeta_de_proyecto() {
        assert_eq!(
            cwd_from_project_dir("-home-dev-Proyectos-demo").as_deref(),
            Some("/home/dev/Proyectos/demo")
        );
        assert_eq!(cwd_from_project_dir("agent-transcripts"), None);
    }

    #[test]
    fn detecta_transcripciones() {
        let p = Cursor::with_roots(vec![PathBuf::from("/h/.cursor/projects")]);
        assert!(p.matches(Path::new(
            "/h/.cursor/projects/-h-x/agent-transcripts/abc/abc.jsonl"
        )));
        assert!(p.matches(Path::new(
            "/h/.cursor/projects/-h-x/agent-transcripts/abc/subagents/s.jsonl"
        )));
        assert!(!p.matches(Path::new("/h/.cursor/projects/-h-x/notas.jsonl")));
    }
}
