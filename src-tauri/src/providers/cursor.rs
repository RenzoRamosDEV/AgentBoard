//! Cursor CLI (`cursor-agent`): transcripciones en
//! `~/.cursor/projects/<carpeta-con-guiones>/agent-transcripts/<id>/<id>.jsonl`
//! (subagentes en `<id>/subagents/*.jsonl`). Cada línea es `{"role": "user"|"assistant",
//! "message": {"content": [{"type": "text"|"tool_use"|"tool-call", ...}]}}`.
//!
//! Cursor no apunta tokens ni modelo en la transcripción: los tokens se estiman por caracteres
//! (≈4 por token) y el modelo queda como `cursor-auto`, sin precio. Sirve para ver actividad,
//! herramientas y turnos; el coste real lo da la suscripción de Cursor.

use super::json::{ListOrLen, ListOrStr, Obj, Str, StrLen};
use super::{prompt_intent, CallRec, Provider, Record, SessionRec, ToolUseRec, TurnRec};
use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use std::borrow::Cow;
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

    fn knows(&self, path: &Path) -> bool {
        self.state.lock().is_ok_and(|s| s.contains_key(path))
    }

    fn reset(&self, path: &Path) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(path);
    }

    fn parse_line(&self, path: &Path, line: &str) -> Result<Vec<Record>> {
        // Sonda: solo el rol. Toda línea avanza el reloj del archivo, produzca filas o no.
        let probe = serde_json::from_str::<Obj<Probe>>(line)?
            .0
            .unwrap_or_default();
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

        match probe.role.as_deref() {
            Some("user") if !st.is_subagent => {
                let user: Obj<UserLine> = serde_json::from_str(line)?;
                let text = user
                    .0
                    .and_then(|u| u.message.0)
                    .map(|m| m.content.text())
                    .unwrap_or_default();
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
                let assistant: Obj<AssistantLine> = serde_json::from_str(line)?;
                let content = assistant
                    .0
                    .and_then(|a| a.message.0)
                    .map(|m| m.content)
                    .unwrap_or_default();
                let mut chars = 0usize;
                let message_id = format!("{}:{}", st.session_id, st.lines);
                let blocks = match content {
                    AssistantContent::Blocks(blocks) => blocks,
                    AssistantContent::Text(len) => {
                        chars += len;
                        Vec::new()
                    }
                    AssistantContent::Other => Vec::new(),
                };
                for b in blocks.iter().filter_map(|b| b.0.as_ref()) {
                    match b.kind.as_deref() {
                        Some("text") | Some("reasoning") => chars += b.text.0.unwrap_or(0),
                        Some("tool_use") | Some("tool-call") => {
                            let raw = b.name.as_deref().or(b.tool_name.as_deref()).unwrap_or("?");
                            let tool = canonical_tool(raw).to_string();
                            // Los argumentos se leen como `Value`: son pocos datos y su tamaño
                            // serializado cuenta para la estimación de tokens.
                            let args = if b.input.is_object() {
                                &b.input
                            } else {
                                &b.args
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
                            let call_id =
                                b.id.as_deref()
                                    .or(b.tool_call_id.as_deref())
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

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Probe<'a> {
    #[serde(borrow)]
    role: Str<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UserLine<'a> {
    #[serde(borrow)]
    message: Obj<UserMessage<'a>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UserMessage<'a> {
    #[serde(borrow)]
    content: UserContent<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UserBlock<'a> {
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
    #[serde(borrow)]
    text: Str<'a>,
}

/// `message.content` del usuario: lista de bloques o texto suelto (un bloque de texto).
#[derive(Debug, Default)]
enum UserContent<'a> {
    Blocks(Vec<Obj<UserBlock<'a>>>),
    Text(Cow<'a, str>),
    #[default]
    Other,
}

impl UserContent<'_> {
    /// Texto de los bloques de texto, unidos por espacios.
    fn text(&self) -> String {
        match self {
            UserContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| b.0.as_ref())
                .filter(|b| b.kind.as_deref() == Some("text"))
                .filter_map(|b| b.text.as_deref())
                .collect::<Vec<_>>()
                .join(" "),
            UserContent::Text(t) => t.to_string(),
            UserContent::Other => String::new(),
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for UserContent<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        match ListOrStr::<Obj<UserBlock<'a>>>::deserialize(d)? {
            ListOrStr::List(blocks) => Ok(UserContent::Blocks(blocks)),
            ListOrStr::Str(text) => Ok(UserContent::Text(text)),
            ListOrStr::Other => Ok(UserContent::Other),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AssistantLine<'a> {
    #[serde(borrow)]
    message: Obj<AssistantMessage<'a>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AssistantMessage<'a> {
    #[serde(borrow)]
    content: AssistantContent<'a>,
}

/// `message.content` del asistente: del texto solo cuenta su longitud.
#[derive(Debug, Default)]
enum AssistantContent<'a> {
    Blocks(Vec<Obj<AssistantBlock<'a>>>),
    Text(usize),
    #[default]
    Other,
}

impl<'de: 'a, 'a> Deserialize<'de> for AssistantContent<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        match ListOrLen::<Obj<AssistantBlock<'a>>>::deserialize(d)? {
            ListOrLen::List(blocks) => Ok(AssistantContent::Blocks(blocks)),
            ListOrLen::Len(len) => Ok(AssistantContent::Text(len)),
            ListOrLen::Other => Ok(AssistantContent::Other),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AssistantBlock<'a> {
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
    text: StrLen,
    #[serde(borrow)]
    name: Str<'a>,
    #[serde(rename = "toolName", borrow)]
    tool_name: Str<'a>,
    input: Value,
    args: Value,
    #[serde(borrow)]
    id: Str<'a>,
    #[serde(rename = "toolCallId", borrow)]
    tool_call_id: Str<'a>,
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

    fn output_tokens(recs: &[Record]) -> i64 {
        recs.iter()
            .find_map(|r| match r {
                Record::Call(c) => Some(c.output_tokens),
                _ => None,
            })
            .unwrap_or(-1)
    }

    #[test]
    fn los_tokens_estimados_no_cambian_al_no_copiar_el_texto() {
        let p = Cursor::with_roots(vec![]);
        let path = Path::new("/h/projects/-h-x/agent-transcripts/a/a.jsonl");
        let texto = "línea con \\\"escapes\\\" y \\u00e9\\n".repeat(20_000);
        let l = format!(
            r#"{{"role":"assistant","message":{{"content":[{{"type":"text","text":"{texto}"}},{{"type":"reasoning","text":"pienso"}},{{"type":"tool_use","name":"shell","input":{{"command":"ls -la","n":1.50,"z":[1,{{"b":null}}]}}}},{{"type":"tool-call","toolName":"read","args":{{"path":"a.rs"}}}},{{"type":"tool_use","name":"grep","input":"no es objeto"}}]}}}}"#
        );
        let v: serde_json::Value = serde_json::from_str(&l).unwrap();
        let blocks = v["message"]["content"].as_array().unwrap();
        let esperado = blocks[0]["text"].as_str().unwrap().len()
            + "pienso".len()
            + blocks[2]["input"].to_string().len()
            + blocks[3]["args"].to_string().len()
            + "null".len();
        let recs = p.parse_line(path, &l).unwrap();
        assert_eq!(output_tokens(&recs), (esperado / CHARS_PER_TOKEN) as i64);
        let targets: Vec<_> = recs
            .iter()
            .filter_map(|r| match r {
                Record::ToolUse(t) => Some((t.tool.as_str(), t.target.as_deref())),
                _ => None,
            })
            .collect();
        assert_eq!(
            targets,
            vec![
                ("Bash", Some("ls -la")),
                ("Read", Some("a.rs")),
                ("Grep", None)
            ]
        );
    }

    #[test]
    fn el_contenido_de_texto_suelto_cuenta_como_un_bloque() {
        let p = Cursor::with_roots(vec![]);
        let path = Path::new("/h/projects/-h-x/agent-transcripts/a/a.jsonl");
        let recs = p
            .parse_line(
                path,
                r#"{"role":"assistant","message":{"content":"12345678"}}"#,
            )
            .unwrap();
        assert_eq!(output_tokens(&recs), 2);
        let u =
            r#"{"role":"user","message":{"content":"<user_query>arregla el fallo</user_query>"}}"#;
        assert!(p
            .parse_line(path, u)
            .unwrap()
            .iter()
            .any(|r| matches!(r, Record::Turn(t) if t.intent == Some("debug"))));
    }

    #[test]
    fn cada_linea_avanza_el_reloj_aunque_no_produzca_filas() {
        let p = Cursor::with_roots(vec![]);
        let path = Path::new("/h/projects/-h-x/agent-transcripts/a/a.jsonl");
        let a = r#"{"role":"assistant","message":{"content":"1234"}}"#;
        let ts = |recs: Vec<Record>| match &recs[0] {
            Record::Session(s) => s.ts,
            other => panic!("{other:?}"),
        };
        let t1 = ts(p.parse_line(path, a).unwrap());
        assert!(p
            .parse_line(path, r#"{"type":"turn_ended","x":[1,2]}"#)
            .unwrap()
            .is_empty());
        assert!(p.parse_line(path, "[1]").unwrap().is_empty());
        assert_eq!(ts(p.parse_line(path, a).unwrap()), t1 + 3000);
        assert!(p.parse_line(path, "{roto").is_err());
    }

    #[test]
    fn la_sonda_solo_lee_el_rol() {
        let probe: Obj<Probe> = serde_json::from_str(
            r#"{"role":"user","message":{"content":[{"type":"text","text":"x"}]}}"#,
        )
        .unwrap();
        assert_eq!(probe.0.unwrap().role.as_deref(), Some("user"));
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
