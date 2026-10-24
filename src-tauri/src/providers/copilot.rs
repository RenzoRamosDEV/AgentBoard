//! GitHub Copilot CLI: `~/.copilot/session-state/<sesión>/events.jsonl` (`COPILOT_HOME` lo cambia).
//! Cada línea es `{"id","timestamp","type","data"}` con tipos `session.start`, `user.message`,
//! `assistant.turn_start`, `assistant.message`, `tool.execution_start|complete`, `session.shutdown`…
//!
//! Solo `assistant.message` trae tokens de salida por respuesta; los de entrada y caché llegan
//! agregados por modelo en `session.shutdown`, así que al cerrar la sesión se reparten entre
//! las respuestas de ese modelo desde el cierre anterior (una sesión reanudada cierra varias veces).

use super::json::{Bool, Int, IntOrStr, Obj, Str};
use super::{
    parse_ts, prompt_intent, CallRec, EventRec, Provider, Record, SessionRec, ToolUseRec, TurnRec,
};
use anyhow::Result;
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
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
    /// Intención del último prompt, a la espera de su turno (el texto no se guarda).
    pending_intent: Option<&'static str>,
    /// Respuestas por modelo desde el último `session.shutdown`: (message_id, ts, tokens de salida).
    pending_calls: HashMap<String, Vec<(String, i64, i64)>>,
    /// Último total acumulado por modelo, para calcular el delta del siguiente cierre.
    last_usage: HashMap<String, Usage>,
}

impl Copilot {
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self {
            roots: Some(roots),
            state: Mutex::default(),
        }
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

/// Envoltura de una línea: tipo y fecha (epoch ms o RFC 3339).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Probe<'a> {
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
    #[serde(borrow)]
    timestamp: IntOrStr<'a>,
}

impl Probe<'_> {
    fn ts(&self) -> Option<i64> {
        match &self.timestamp {
            IntOrStr::Int(ms) => *ms,
            IntOrStr::Str(s) => parse_ts(s),
            IntOrStr::Other => None,
        }
    }

    /// ¿Puede la línea producir filas o cambiar el estado del archivo? (Derivado de `parse_line`.)
    fn is_useful(&self) -> bool {
        matches!(
            self.kind.as_deref(),
            Some(
                "session.start"
                    | "session.resume"
                    | "session.model_change"
                    | "user.message"
                    | "assistant.turn_start"
                    | "assistant.message"
                    | "tool.execution_start"
                    | "subagent.started"
                    | "skill.invoked"
                    | "tool.execution_complete"
                    | "subagent.completed"
                    | "session.shutdown"
                    | "session.compaction_complete"
                    | "abort"
            )
        )
    }
}

/// Línea útil: su `id` y los campos usados de `data` (sin textos de mensajes ni resultados).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Line<'a> {
    #[serde(borrow)]
    id: Str<'a>,
    #[serde(borrow)]
    data: Obj<Data<'a>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Data<'a> {
    #[serde(borrow)]
    session_id: Str<'a>,
    #[serde(borrow)]
    selected_model: Str<'a>,
    #[serde(borrow)]
    current_model: Str<'a>,
    #[serde(borrow)]
    context: Obj<Context<'a>>,
    #[serde(borrow)]
    new_model: Str<'a>,
    #[serde(borrow)]
    model: Str<'a>,
    #[serde(borrow)]
    turn_id: Str<'a>,
    #[serde(borrow)]
    message_id: Str<'a>,
    output_tokens: Int,
    #[serde(borrow)]
    tool_name: Str<'a>,
    #[serde(borrow)]
    name: Str<'a>,
    #[serde(borrow)]
    arguments: Obj<Arguments<'a>>,
    #[serde(borrow)]
    agent_name: Str<'a>,
    #[serde(borrow)]
    agent_type: Str<'a>,
    #[serde(borrow)]
    subagent_type: Str<'a>,
    #[serde(borrow)]
    agent: Str<'a>,
    #[serde(borrow)]
    skill_name: Str<'a>,
    #[serde(borrow)]
    skill: Str<'a>,
    #[serde(borrow)]
    tool_call_id: Str<'a>,
    #[serde(borrow)]
    subagent_id: Str<'a>,
    #[serde(borrow)]
    agent_id: Str<'a>,
    #[serde(borrow)]
    id: Str<'a>,
    success: Bool,
    #[serde(borrow)]
    error: Str<'a>,
    #[serde(borrow)]
    model_metrics: Obj<BTreeMap<Cow<'a, str>, Obj<Metric>>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Context<'a> {
    #[serde(borrow)]
    cwd: Str<'a>,
    #[serde(borrow)]
    branch: Str<'a>,
}

/// Argumentos de una herramienta: objetivo y nombre del subagente o la skill.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Arguments<'a> {
    #[serde(borrow)]
    command: Str<'a>,
    #[serde(borrow)]
    path: Str<'a>,
    #[serde(borrow)]
    file_path: Str<'a>,
    #[serde(borrow)]
    pattern: Str<'a>,
    #[serde(borrow)]
    query: Str<'a>,
    #[serde(borrow)]
    url: Str<'a>,
    #[serde(borrow)]
    description: Str<'a>,
    #[serde(rename = "agentName", borrow)]
    agent_name: Str<'a>,
    #[serde(rename = "agentType", borrow)]
    agent_type: Str<'a>,
    #[serde(rename = "subagentType", borrow)]
    subagent_type: Str<'a>,
    #[serde(borrow)]
    name: Str<'a>,
    #[serde(borrow)]
    agent: Str<'a>,
    #[serde(rename = "skillName", borrow)]
    skill_name: Str<'a>,
    #[serde(borrow)]
    skill: Str<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Metric {
    usage: Obj<MetricUsage>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct MetricUsage {
    input_tokens: Int,
    output_tokens: Int,
    cache_read_tokens: Int,
    cache_write_tokens: Int,
    reasoning_tokens: Int,
}

impl From<&MetricUsage> for Usage {
    fn from(u: &MetricUsage) -> Self {
        Self {
            input: u.input_tokens.0,
            output: u.output_tokens.0,
            cache_read: u.cache_read_tokens.0,
            cache_write: u.cache_write_tokens.0,
            reasoning: u.reasoning_tokens.0,
        }
    }
}

/// Un `user.message`: solo su texto, para la intención.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UserLine<'a> {
    #[serde(borrow)]
    data: Obj<UserData<'a>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UserData<'a> {
    #[serde(borrow)]
    content: Str<'a>,
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
        Self::home()
            .map(|h| vec![h.join("session-state")])
            .unwrap_or_default()
    }

    fn installed(&self) -> bool {
        Self::home().is_some_and(|h| h.is_dir()) || super::on_path("copilot")
    }

    fn matches(&self, path: &Path) -> bool {
        path.file_name().is_some_and(|f| f == "events.jsonl")
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
        // Sonda: tipo y fecha; los tipos que no producen filas ni cambian el estado se descartan
        // sin leer su contenido.
        let Obj(Some(probe)) = serde_json::from_str::<Obj<Probe>>(line)? else {
            return Ok(vec![]);
        };
        let Some(ts) = probe.ts() else {
            return Ok(vec![]);
        };
        let mut states = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let st = states.entry(path.to_path_buf()).or_default();
        if st.session_id.is_empty() {
            // La carpeta de la sesión lleva su id; `session.start` lo confirma.
            st.session_id = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|f| f.to_str())
                .unwrap_or("")
                .to_string();
        }
        if !probe.is_useful() {
            return Ok(vec![]);
        }
        let kind = probe.kind.as_deref().unwrap_or("");
        if kind == "user.message" {
            // Solo la intención: el texto del prompt no se guarda.
            let user: Obj<UserLine> = serde_json::from_str(line)?;
            let text = user
                .0
                .as_ref()
                .and_then(|u| u.data.0.as_ref())
                .and_then(|d| d.content.as_deref())
                .unwrap_or("");
            st.pending_intent = prompt_intent(text);
            return Ok(vec![]);
        }
        let parsed: Obj<Line> = serde_json::from_str(line)?;
        let empty_line = Line::default();
        let line_rec = parsed.0.as_ref().unwrap_or(&empty_line);
        let empty = Data::default();
        let d = line_rec.data.0.as_ref().unwrap_or(&empty);
        let no_args = Arguments::default();
        let args = d.arguments.0.as_ref().unwrap_or(&no_args);
        let mut out = Vec::new();

        match kind {
            "session.start" | "session.resume" => {
                if let Some(id) = d.session_id.as_deref() {
                    st.session_id = id.to_string();
                }
                if let Some(m) = d.selected_model.as_deref().or(d.current_model.as_deref()) {
                    st.model = m.to_string();
                }
                let context = d.context.0.as_ref();
                if let Some(c) = context.and_then(|c| c.cwd.as_deref()) {
                    st.cwd = Some(c.to_string());
                }
                if let Some(b) = context.and_then(|c| c.branch.as_deref()) {
                    st.branch = Some(b.to_string());
                }
            }
            "session.model_change" => {
                if let Some(m) = d
                    .new_model
                    .as_deref()
                    .or(d.selected_model.as_deref())
                    .or(d.model.as_deref())
                {
                    st.model = m.to_string();
                }
            }
            "assistant.turn_start" => {
                st.turns += 1;
                let id = d
                    .turn_id
                    .as_deref()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{}:{}", st.session_id, st.turns));
                st.turn_id = Some(id.clone());
                out.push(Record::Turn(TurnRec {
                    id,
                    session_id: st.session_id.clone(),
                    ts,
                    intent: st.pending_intent.take(),
                }));
            }
            "assistant.message" => {
                if let Some(m) = d.model.as_deref() {
                    st.model = m.to_string();
                }
                if let Some(t) = d.turn_id.as_deref() {
                    st.turn_id = Some(t.to_string());
                }
                let message_id = d
                    .message_id
                    .as_deref()
                    .or(line_rec.id.as_deref())
                    .unwrap_or("")
                    .to_string();
                let output = d.output_tokens.0;
                if !message_id.is_empty() {
                    let model = crate::pricing::normalize_model(&st.model);
                    st.pending_calls.entry(model.clone()).or_default().push((
                        message_id.clone(),
                        ts,
                        output,
                    ));
                    out.push(call(
                        &st.session_id,
                        message_id,
                        ts,
                        model,
                        Usage {
                            output,
                            ..Default::default()
                        },
                    ));
                }
            }
            "tool.execution_start" | "subagent.started" | "skill.invoked" => {
                let raw = d.tool_name.as_deref().or(d.name.as_deref()).unwrap_or(
                    if kind == "skill.invoked" {
                        "skill"
                    } else {
                        "task"
                    },
                );
                let tool = if kind == "subagent.started" {
                    "Agent".to_string()
                } else {
                    canonical_tool(raw).to_string()
                };
                let target = [
                    &args.command,
                    &args.path,
                    &args.file_path,
                    &args.pattern,
                    &args.query,
                    &args.url,
                    &args.description,
                ]
                .iter()
                .find_map(|k| k.as_deref())
                .map(|s| s.chars().take(500).collect::<String>());
                let detail = match tool.as_str() {
                    "Agent" => Some(
                        [
                            (&d.agent_name, &args.agent_name),
                            (&d.agent_type, &args.agent_type),
                            (&d.subagent_type, &args.subagent_type),
                            (&d.name, &args.name),
                            (&d.agent, &args.agent),
                        ]
                        .iter()
                        .find_map(|(a, b)| a.as_deref().or(b.as_deref()))
                        .unwrap_or("general-purpose")
                        .to_string(),
                    ),
                    "Skill" => [
                        (&d.skill_name, &args.skill_name),
                        (&d.name, &args.name),
                        (&d.skill, &args.skill),
                    ]
                    .iter()
                    .find_map(|(a, b)| a.as_deref().or(b.as_deref()))
                    .map(str::to_string),
                    _ => None,
                };
                let call_id = tool_call_id(d)
                    .or(line_rec.id.as_deref())
                    .unwrap_or("")
                    .to_string();
                if !call_id.is_empty() {
                    if let Some(t) = d.turn_id.as_deref() {
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
                if let Some(call_id) = tool_call_id(d) {
                    out.push(Record::ToolResult {
                        call_id: call_id.to_string(),
                        ts,
                        is_error: d.success.0 == Some(false) || d.error.0.is_some(),
                        agent_id: None,
                    });
                }
            }
            "session.shutdown" => {
                // Reparte el delta de tokens de cada modelo entre sus respuestas pendientes.
                if let Some(metrics) = d.model_metrics.0.as_ref() {
                    for (raw_model, m) in metrics {
                        let model = crate::pricing::normalize_model(raw_model);
                        let total =
                            m.0.as_ref()
                                .and_then(|m| m.usage.0.as_ref())
                                .map(Usage::from)
                                .unwrap_or_default();
                        let prev = st
                            .last_usage
                            .insert(model.clone(), total)
                            .unwrap_or_default();
                        let delta = total.delta(prev);
                        let calls = st.pending_calls.remove(&model).unwrap_or_default();
                        if calls.is_empty() {
                            // Sin respuestas registradas: una llamada agregada para no perder el coste.
                            if delta != Usage::default() {
                                out.push(call(
                                    &st.session_id,
                                    format!("{}:{model}:{ts}", st.session_id),
                                    ts,
                                    model,
                                    delta,
                                ));
                            }
                            continue;
                        }
                        let n = calls.len() as i64;
                        let out_sum: i64 = calls.iter().map(|c| c.2).sum();
                        for (i, (id, cts, output)) in calls.iter().enumerate() {
                            let share = |v: i64| v / n + if (i as i64) < v % n { 1 } else { 0 };
                            // Si el total de salida supera lo ya contado, el resto va al último.
                            let extra_out = if i + 1 == calls.len() {
                                (delta.output - out_sum).max(0)
                            } else {
                                0
                            };
                            out.push(call(
                                &st.session_id,
                                id.clone(),
                                *cts,
                                model.clone(),
                                Usage {
                                    input: share(delta.input),
                                    output: output + extra_out,
                                    cache_read: share(delta.cache_read),
                                    cache_write: share(delta.cache_write),
                                    reasoning: share(delta.reasoning),
                                },
                            ));
                        }
                    }
                }
            }
            "session.compaction_complete" => out.push(Record::Event(EventRec {
                session_id: st.session_id.clone(),
                ts,
                kind: "compaction".into(),
                payload_json: None,
            })),
            "abort" => out.push(Record::Event(EventRec {
                session_id: st.session_id.clone(),
                ts,
                kind: "interruption".into(),
                payload_json: None,
            })),
            _ => {}
        }

        if !out.is_empty() && !st.session_id.is_empty() {
            out.insert(
                0,
                Record::Session(SessionRec {
                    id: st.session_id.clone(),
                    cwd: st.cwd.clone(),
                    git_branch: st.branch.clone(),
                    ts,
                    is_subagent: false,
                }),
            );
        }
        Ok(out)
    }
}

/// Id de la llamada a una herramienta o subagente, por orden de preferencia.
fn tool_call_id<'d>(d: &'d Data) -> Option<&'d str> {
    [&d.tool_call_id, &d.subagent_id, &d.agent_id, &d.id]
        .iter()
        .find_map(|k| k.as_deref())
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

    fn probe(line: &str) -> Probe<'_> {
        serde_json::from_str::<Obj<Probe>>(line).unwrap().0.unwrap()
    }

    #[test]
    fn fecha_numerica_o_texto() {
        assert_eq!(
            probe(r#"{ "timestamp": 1770000000100 }"#).ts(),
            Some(1_770_000_000_100)
        );
        assert_eq!(
            probe(r#"{ "timestamp": "2026-09-25T09:57:51.467Z" }"#).ts(),
            Some(1_790_330_271_467)
        );
        assert_eq!(probe(r#"{ "timestamp": 1.5 }"#).ts(), None);
        assert_eq!(probe(r#"{ "timestamp": 18446744073709551615 }"#).ts(), None);
        assert_eq!(probe(r#"{ "timestamp": "ayer" }"#).ts(), None);
        assert_eq!(probe("{}").ts(), None);
    }

    #[test]
    fn la_sonda_solo_deja_pasar_los_tipos_que_producen_filas_o_estado() {
        let useful = |t: &str| probe(&format!(r#"{{"type":"{t}","timestamp":1}}"#)).is_useful();
        for t in [
            "session.start",
            "session.resume",
            "session.model_change",
            "user.message",
            "assistant.turn_start",
            "assistant.message",
            "tool.execution_start",
            "subagent.started",
            "skill.invoked",
            "tool.execution_complete",
            "subagent.completed",
            "session.shutdown",
            "session.compaction_complete",
            "abort",
        ] {
            assert!(useful(t), "{t}");
        }
        for t in [
            "assistant.reasoning",
            "session.info",
            "hook.start",
            "",
            "otro",
        ] {
            assert!(!useful(t), "{t}");
        }
    }

    #[test]
    fn una_linea_descartada_fija_la_sesion_de_la_carpeta() {
        let p = Copilot::with_roots(vec![]);
        let path = Path::new("/h/s/sesion-1/events.jsonl");
        let razonamiento =
            r#"{"type":"assistant.reasoning","timestamp":1,"data":{"content":"pienso"}}"#;
        assert!(p.parse_line(path, razonamiento).unwrap().is_empty());
        assert!(p.knows(path));
        let fin = r#"{"type":"abort","timestamp":2,"data":{}}"#;
        let recs = p.parse_line(path, fin).unwrap();
        assert!(
            matches!(&recs[0], Record::Session(s) if s.id == "sesion-1"),
            "{recs:?}"
        );
    }

    #[test]
    fn el_resultado_de_una_herramienta_no_se_lee() {
        let p = Copilot::with_roots(vec![]);
        let path = Path::new("/h/s/sesion-1/events.jsonl");
        let salida = "x\\n".repeat(100_000);
        let fin = format!(
            r#"{{"type":"tool.execution_complete","timestamp":3,"data":{{"toolCallId":"c1","success":false,"result":{{"content":"{salida}"}}}}}}"#
        );
        let recs = p.parse_line(path, &fin).unwrap();
        assert!(recs.contains(&Record::ToolResult {
            call_id: "c1".into(),
            ts: 3,
            is_error: true,
            agent_id: None
        }));
        let error = r#"{"type":"tool.execution_complete","timestamp":3,"data":{"toolCallId":"c2","error":"boom"}}"#;
        assert!(p.parse_line(path, error).unwrap().iter().any(
            |r| matches!(r, Record::ToolResult { call_id, is_error: true, .. } if call_id == "c2")
        ));
        let objeto = r#"{"type":"tool.execution_complete","timestamp":3,"data":{"toolCallId":"c3","error":{"m":1}}}"#;
        assert!(p.parse_line(path, objeto).unwrap().iter().any(
            |r| matches!(r, Record::ToolResult { call_id, is_error: false, .. } if call_id == "c3")
        ));
    }

    #[test]
    fn delta_de_uso() {
        let a = Usage {
            input: 100,
            output: 10,
            cache_read: 50,
            cache_write: 5,
            reasoning: 1,
        };
        let b = Usage {
            input: 250,
            output: 30,
            cache_read: 50,
            cache_write: 20,
            reasoning: 1,
        };
        assert_eq!(
            b.delta(a),
            Usage {
                input: 150,
                output: 20,
                cache_read: 0,
                cache_write: 15,
                reasoning: 0
            }
        );
    }
}
