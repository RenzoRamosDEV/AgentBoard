//! Sesiones de Claude Code: `~/.claude/projects/<proyecto>/<sesión>.jsonl`.

use super::json::{Bool, Int, List, Obj, Str};
use super::{
    classify_tool, parse_ts, prompt_intent, CallRec, EventRec, Provider, Record, SessionRec,
    ToolUseRec, TurnRec,
};
use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use std::borrow::Cow;
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
        // Sonda: solo la envoltura; `message` y el resto se saltan sin interpretarlos.
        let Obj(Some(probe)) = serde_json::from_str::<Obj<Probe>>(line)? else {
            return Ok(vec![]);
        };
        let Some(session_id) = probe.session_id.as_deref() else {
            return Ok(vec![]);
        };
        let Some(ts) = probe.timestamp.as_deref().and_then(parse_ts) else {
            return Ok(vec![]);
        };
        let cwd = {
            let mut map = self.first_cwd.lock().unwrap_or_else(|e| e.into_inner());
            match (map.get(path), probe.cwd.as_deref()) {
                (Some(first), _) => Some(first.clone()),
                (None, Some(c)) => {
                    map.insert(path.to_path_buf(), c.to_string());
                    Some(c.to_string())
                }
                (None, None) => None,
            }
        };
        let mut out = vec![Record::Session(SessionRec {
            id: session_id.to_string(),
            cwd,
            git_branch: probe
                .git_branch
                .as_deref()
                .filter(|b| !b.is_empty())
                .map(str::to_string),
            ts,
            is_subagent: false,
        })];

        match probe.kind.as_deref() {
            Some("assistant") => {
                if let Obj(Some(a)) = serde_json::from_str(line)? {
                    parse_assistant(&a, session_id, ts, &mut out);
                }
            }
            Some("user") => {
                if let Obj(Some(u)) = serde_json::from_str(line)? {
                    parse_user(&u, session_id, ts, &mut out);
                }
            }
            Some("system") if probe.subtype.as_deref() == Some("compact_boundary") => {
                // Poco frecuente: se conserva el formato de `Value` para el payload.
                let v: Value = serde_json::from_str(line)?;
                out.push(Record::Event(EventRec {
                    session_id: session_id.to_string(),
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

/// Envoltura común de todas las líneas.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Probe<'a> {
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
    #[serde(borrow)]
    subtype: Str<'a>,
    #[serde(rename = "sessionId", borrow)]
    session_id: Str<'a>,
    #[serde(borrow)]
    timestamp: Str<'a>,
    #[serde(borrow)]
    cwd: Str<'a>,
    #[serde(rename = "gitBranch", borrow)]
    git_branch: Str<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AssistantLine<'a> {
    #[serde(borrow)]
    message: Obj<AssistantMsg<'a>>,
    #[serde(borrow)]
    uuid: Str<'a>,
    #[serde(rename = "isSidechain")]
    is_sidechain: Bool,
    #[serde(rename = "agentId", borrow)]
    agent_id: Str<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AssistantMsg<'a> {
    #[serde(borrow)]
    model: Str<'a>,
    #[serde(borrow)]
    id: Str<'a>,
    /// Solo los campos de los usos de herramientas: el texto de los bloques no se declara.
    #[serde(borrow)]
    content: List<Obj<ToolBlock<'a>>>,
    usage: Obj<Usage>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ToolBlock<'a> {
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
    #[serde(borrow)]
    id: Str<'a>,
    #[serde(borrow)]
    name: Str<'a>,
    #[serde(borrow)]
    input: Obj<ToolInput<'a>>,
}

/// Campos de la entrada de una herramienta que dan su objetivo, su skill o su subagente.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ToolInput<'a> {
    #[serde(borrow)]
    file_path: Str<'a>,
    #[serde(borrow)]
    command: Str<'a>,
    #[serde(borrow)]
    path: Str<'a>,
    #[serde(borrow)]
    notebook_path: Str<'a>,
    #[serde(borrow)]
    url: Str<'a>,
    #[serde(borrow)]
    pattern: Str<'a>,
    #[serde(borrow)]
    query: Str<'a>,
    #[serde(borrow)]
    description: Str<'a>,
    #[serde(borrow)]
    skill: Str<'a>,
    #[serde(borrow)]
    subagent_type: Str<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Usage {
    input_tokens: Int,
    output_tokens: Int,
    cache_read_input_tokens: Int,
    cache_creation_input_tokens: Int,
    cache_creation: Obj<CacheCreation>,
    output_tokens_details: Obj<OutputDetails>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct CacheCreation {
    ephemeral_1h_input_tokens: Int,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OutputDetails {
    thinking_tokens: Int,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UserLine<'a> {
    #[serde(borrow)]
    message: Obj<UserMsg<'a>>,
    #[serde(rename = "isSidechain")]
    is_sidechain: Bool,
    #[serde(rename = "isMeta")]
    is_meta: Bool,
    #[serde(rename = "promptId", borrow)]
    prompt_id: Str<'a>,
    #[serde(rename = "toolUseResult", borrow)]
    tool_use_result: Obj<ToolUseResult<'a>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UserMsg<'a> {
    #[serde(borrow)]
    content: Content<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ToolUseResult<'a> {
    #[serde(rename = "agentId", borrow)]
    agent_id: Str<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UserBlock<'a> {
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
    #[serde(borrow)]
    tool_use_id: Str<'a>,
    is_error: Bool,
    #[serde(borrow)]
    text: Str<'a>,
}

/// `message.content` de un usuario: texto suelto o lista de bloques.
#[derive(Debug, Default)]
enum Content<'a> {
    Text(Cow<'a, str>),
    Blocks(Vec<Obj<UserBlock<'a>>>),
    #[default]
    Other,
}

impl<'de: 'a, 'a> Deserialize<'de> for Content<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        // Se reutilizan los tipos tolerantes: primero como lista; si no, como cadena.
        struct V<'a>(std::marker::PhantomData<&'a ()>);
        impl<'de: 'a, 'a> serde::de::Visitor<'de> for V<'a> {
            type Value = Content<'a>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("cualquier valor JSON")
            }
            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                v: &'de str,
            ) -> std::result::Result<Content<'a>, E> {
                Ok(Content::Text(Cow::Borrowed(v)))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> std::result::Result<Content<'a>, E> {
                Ok(Content::Text(Cow::Owned(v.to_owned())))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                v: String,
            ) -> std::result::Result<Content<'a>, E> {
                Ok(Content::Text(Cow::Owned(v)))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                seq: A,
            ) -> std::result::Result<Content<'a>, A::Error> {
                let List(blocks) =
                    List::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))?;
                Ok(Content::Blocks(blocks))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> std::result::Result<Content<'a>, A::Error> {
                serde::de::IgnoredAny::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))?;
                Ok(Content::Other)
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                _: bool,
            ) -> std::result::Result<Content<'a>, E> {
                Ok(Content::Other)
            }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<Content<'a>, E> {
                Ok(Content::Other)
            }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<Content<'a>, E> {
                Ok(Content::Other)
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<Content<'a>, E> {
                Ok(Content::Other)
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Content<'a>, E> {
                Ok(Content::Other)
            }
        }
        d.deserialize_any(V(std::marker::PhantomData))
    }
}

fn parse_assistant(line: &AssistantLine, session_id: &str, ts: i64, out: &mut Vec<Record>) {
    let empty = AssistantMsg::default();
    let msg = line.message.0.as_ref().unwrap_or(&empty);
    let model = msg.model.as_deref().unwrap_or("");
    let message_id = msg
        .id
        .as_deref()
        .or_else(|| line.uuid.as_deref())
        .unwrap_or_default()
        .to_string();
    if message_id.is_empty() {
        return;
    }

    let mut activity = None;
    let no_input = ToolInput::default();
    for b in msg.content.0.iter().filter_map(|b| b.0.as_ref()) {
        if b.kind.as_deref() != Some("tool_use") {
            continue;
        }
        let tool = b.name.as_deref().unwrap_or("?").to_string();
        let input = b.input.0.as_ref().unwrap_or(&no_input);
        let target = tool_target(input);
        let detail = match tool.as_str() {
            "Skill" => input.skill.as_deref().map(str::to_string),
            "Agent" | "Task" => Some(
                input
                    .subagent_type
                    .as_deref()
                    .unwrap_or("general-purpose")
                    .to_string(),
            ),
            _ => None,
        };
        activity = Some(classify_tool(&tool, target.as_deref()).to_string());
        if let Some(call_id) = b.id.as_deref() {
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

    // Mensajes sintéticos (errores locales) no tienen coste real.
    if let (Some(usage), false) = (
        msg.usage.0.as_ref(),
        model.is_empty() || model == "<synthetic>",
    ) {
        out.insert(
            1,
            Record::Call(CallRec {
                message_id,
                session_id: session_id.to_string(),
                ts,
                model: crate::pricing::normalize_model(model),
                input_tokens: usage.input_tokens.0,
                output_tokens: usage.output_tokens.0,
                cache_read: usage.cache_read_input_tokens.0,
                cache_write: usage.cache_creation_input_tokens.0,
                cache_write_1h: usage
                    .cache_creation
                    .0
                    .as_ref()
                    .map_or(0, |c| c.ephemeral_1h_input_tokens.0),
                reasoning_tokens: usage
                    .output_tokens_details
                    .0
                    .as_ref()
                    .map_or(0, |d| d.thinking_tokens.0),
                activity,
                is_sidechain: line.is_sidechain.0.unwrap_or(false),
                agent_id: line.agent_id.as_deref().map(str::to_string),
                cost_reported: None,
            }),
        );
    }
}

fn parse_user(line: &UserLine, session_id: &str, ts: i64, out: &mut Vec<Record>) {
    let sidechain = line.is_sidechain.0.unwrap_or(false);
    let content = line.message.0.as_ref().map(|m| &m.content);
    // El texto del prompt humano: se usa para la intención y se descarta. Prestado de la
    // línea salvo que haya que unir varios bloques.
    let mut prompt: Option<Cow<str>> = None;
    match content {
        Some(Content::Blocks(blocks)) => {
            for b in blocks.iter().filter_map(|b| b.0.as_ref()) {
                match b.kind.as_deref() {
                    Some("tool_result") => {
                        if let Some(id) = b.tool_use_id.as_deref() {
                            out.push(Record::ToolResult {
                                call_id: id.to_string(),
                                ts,
                                is_error: b.is_error.0.unwrap_or(false),
                                agent_id: line
                                    .tool_use_result
                                    .0
                                    .as_ref()
                                    .and_then(|r| r.agent_id.as_deref())
                                    .map(str::to_string),
                            });
                        }
                    }
                    Some("text") if is_interruption(b.text.as_deref()) => {
                        out.push(interruption(session_id, ts));
                    }
                    Some("text") => {
                        let text = b.text.as_deref().unwrap_or("");
                        prompt = Some(match prompt {
                            None => Cow::Borrowed(text),
                            Some(prev) => Cow::Owned(prev.into_owned() + text),
                        });
                    }
                    _ => {}
                }
            }
        }
        Some(Content::Text(text)) if is_interruption(Some(text)) => {
            out.push(interruption(session_id, ts));
        }
        Some(Content::Text(text)) => prompt = Some(Cow::Borrowed(text)),
        _ => {}
    }

    // Las líneas de un turno comparten promptId; las de subagentes no abren turno propio.
    if let (Some(id), false) = (line.prompt_id.as_deref(), sidechain) {
        let is_human = !line.is_meta.0.unwrap_or(false);
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
fn tool_target(input: &ToolInput) -> Option<String> {
    [
        &input.file_path,
        &input.command,
        &input.path,
        &input.notebook_path,
        &input.url,
        &input.pattern,
        &input.query,
        &input.description,
    ]
    .iter()
    .find_map(|k| k.as_deref())
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
    fn la_sonda_lee_la_envoltura_prestada_y_descarta_sin_sesion_o_fecha() {
        use std::borrow::Cow;
        let line = r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-09-25T10:00:00Z","message":{"content":7,"usage":"roto"}}"#;
        let probe: Probe = serde_json::from_str(line).unwrap();
        assert!(matches!(probe.session_id.0, Some(Cow::Borrowed("s1"))));
        assert!(matches!(probe.kind.0, Some(Cow::Borrowed("assistant"))));
        for sin in [
            r#"{"type":"assistant","timestamp":"2026-09-25T10:00:00Z","message":{"content":[{"type":"tool_use","id":"t","name":"Bash","input":{}}],"usage":{"input_tokens":1}}}"#,
            r#"{"type":"user","sessionId":"s1","timestamp":"no es fecha","promptId":"p","message":{"content":"hola"}}"#,
            r#"{"type":"user","sessionId":"s1","message":{"content":"hola"}}"#,
        ] {
            assert!(parse(sin).is_empty(), "{sin}");
        }
    }

    #[test]
    fn tipos_inesperados_se_tratan_como_ausentes() {
        assert!(parse("[1,2]").is_empty());
        assert!(parse("\"texto\"").is_empty());
        let raro = r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-09-25T10:00:00Z","isSidechain":"si","cwd":7,
          "message":{"model":"claude-opus-5-5","id":"m","content":"texto suelto","usage":{"input_tokens":"x","output_tokens":2.5,"cache_read_input_tokens":3}}}"#;
        let recs = parse(raro);
        let Record::Session(s) = &recs[0] else {
            panic!("{recs:?}")
        };
        assert_eq!(s.cwd, None);
        let Record::Call(c) = &recs[1] else {
            panic!("{recs:?}")
        };
        assert_eq!(
            (
                c.input_tokens,
                c.output_tokens,
                c.cache_read,
                c.is_sidechain
            ),
            (0, 0, 3, false)
        );
        assert_eq!(recs.len(), 2, "sin herramientas: content no es una lista");
    }

    #[test]
    fn prompt_largo_con_escapes_da_la_misma_intencion_sin_guardar_el_texto() {
        let texto = "Arregla el \\\"bug\\\" de la ingesta\\n".repeat(100_000);
        let u = format!(
            r#"{{"type":"user","sessionId":"s1","promptId":"p1","timestamp":"2026-09-25T10:00:00Z","message":{{"role":"user","content":"{texto}"}}}}"#
        );
        assert!(u.len() > 3_000_000);
        let recs = parse(&u);
        assert!(recs.contains(&Record::Turn(TurnRec {
            id: "p1".into(),
            session_id: "s1".into(),
            ts: parse_ts("2026-09-25T10:00:00Z").unwrap(),
            intent: Some("debug"),
        })));
        let bloques = u.replace(
            &format!(r#""content":"{texto}""#),
            &format!(
                r#""content":[{{"type":"text","text":"{texto}"}},{{"type":"text","text":"y más"}}]"#
            ),
        );
        assert!(parse(&bloques)
            .iter()
            .any(|r| matches!(r, Record::Turn(t) if t.intent == Some("debug"))));
    }

    #[test]
    fn el_texto_sin_escapes_se_lee_prestado() {
        use std::borrow::Cow;
        let u = r#"{"message":{"role":"user","content":[{"type":"text","text":"hola mundo"}]}}"#;
        let line: UserLine = serde_json::from_str(u).unwrap();
        let blocks = match line.message.0.unwrap().content {
            Content::Blocks(b) => b,
            other => panic!("{other:?}"),
        };
        assert!(matches!(
            &blocks[0].0.as_ref().unwrap().text.0,
            Some(Cow::Borrowed("hola mundo"))
        ));
    }

    #[test]
    fn respeta_claude_config_dir() {
        let p = ClaudeCode::with_roots(vec![PathBuf::from("/opt/claude/projects")]);
        assert!(p.matches(Path::new("/opt/claude/projects/a/b.jsonl")));
        assert!(!p.matches(Path::new("/otra/b.jsonl")));
    }
}
