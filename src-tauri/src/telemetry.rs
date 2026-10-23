//! Trazas del backend: activables con `AGENTBOARD_LOG`, en `warn` por defecto y siempre a stderr.

use tracing::Subscriber;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;

pub const ENV_VAR: &str = "AGENTBOARD_LOG";
const DEFAULT_LEVEL: &str = "warn";

fn parse(env: Option<&str>) -> Option<Result<EnvFilter, String>> {
    let value = env.map(str::trim).filter(|v| !v.is_empty())?;
    Some(EnvFilter::try_new(value).map_err(|e| e.to_string()))
}

pub fn filter_from(env: Option<&str>) -> EnvFilter {
    match parse(env) {
        Some(Ok(filter)) => filter,
        _ => EnvFilter::new(DEFAULT_LEVEL),
    }
}

pub fn subscriber_with_writer<W>(filter: EnvFilter, writer: W) -> impl Subscriber + Send + Sync
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::CLOSE)
        .with_ansi(false)
        .with_writer(writer)
        .finish()
}

/// Instala las trazas leyendo `AGENTBOARD_LOG`. Si ya hay un subscriber global no hace nada.
pub fn init() {
    let env = std::env::var(ENV_VAR).ok();
    // stdout queda reservado (p. ej. para el JSON-RPC del servidor MCP).
    let subscriber = subscriber_with_writer(filter_from(env.as_deref()), std::io::stderr);
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        return;
    }
    if let Some(Err(e)) = parse(env.as_deref()) {
        tracing::warn!(
            valor = env.as_deref().unwrap_or_default(),
            "{ENV_VAR} no válida, se usa {DEFAULT_LEVEL}: {e}"
        );
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    /// Escritor en memoria para capturar lo que se escribiría en stderr.
    #[derive(Clone, Default)]
    pub(crate) struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl Buffer {
        pub(crate) fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
        }
    }

    impl Write for Buffer {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'w> MakeWriter<'w> for Buffer {
        type Writer = Buffer;
        fn make_writer(&'w self) -> Self::Writer {
            self.clone()
        }
    }

    /// Ejecuta `f` con las trazas configuradas como con `AGENTBOARD_LOG=env` y devuelve la salida.
    pub(crate) fn captured(env: Option<&str>, f: impl FnOnce()) -> String {
        let buf = Buffer::default();
        tracing::subscriber::with_default(subscriber_with_writer(filter_from(env), buf.clone()), f);
        buf.text()
    }

    fn emit_all_levels() {
        tracing::debug_span!("span_de_prueba").in_scope(|| {});
        tracing::info!("mensaje_info");
        tracing::warn!("mensaje_warn");
    }

    #[test]
    fn sin_variable_solo_salen_avisos() {
        let out = captured(None, emit_all_levels);
        assert!(out.contains("mensaje_warn"), "{out}");
        assert!(!out.contains("mensaje_info"), "{out}");
        assert!(!out.contains("span_de_prueba"), "{out}");
    }

    #[test]
    fn variable_vacia_equivale_a_sin_variable() {
        let out = captured(Some("  "), emit_all_levels);
        assert!(out.contains("mensaje_warn"), "{out}");
        assert!(!out.contains("mensaje_info"), "{out}");
    }

    #[test]
    fn con_debug_el_cierre_del_span_escribe_nombre_y_duracion() {
        let out = captured(Some("debug"), emit_all_levels);
        assert!(out.contains("span_de_prueba"), "{out}");
        assert!(out.contains("time.busy"), "{out}");
        assert!(out.contains("mensaje_info"), "{out}");
    }

    #[test]
    fn directiva_por_modulo_filtra_el_resto() {
        let out = captured(Some("otro_modulo=debug"), emit_all_levels);
        assert!(!out.contains("span_de_prueba"), "{out}");
        assert!(!out.contains("mensaje_warn"), "{out}");
    }

    #[test]
    fn valor_invalido_se_comporta_como_warn() {
        let out = captured(Some("agentboard_lib=nivel_raro"), emit_all_levels);
        assert!(out.contains("mensaje_warn"), "{out}");
        assert!(!out.contains("mensaje_info"), "{out}");
        assert!(!out.contains("span_de_prueba"), "{out}");
    }

    #[test]
    fn la_salida_no_lleva_colores() {
        let out = captured(None, emit_all_levels);
        assert!(!out.contains('\u{1b}'), "{out:?}");
    }

    #[test]
    fn init_es_idempotente() {
        init();
        init();
    }
}
