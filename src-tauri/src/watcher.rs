//! Vigilante de archivos: observa las carpetas de logs de los agentes y, al cambiar algo,
//! pide una relectura al hilo de ingesta (los offsets de `file_state` evitan duplicar).

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use crate::ingest_thread::{IngestHandle, Origen};
use crate::providers::Provider;
use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Espera tras el primer cambio antes de releer, para agrupar ráfagas de escrituras.
const DEBOUNCE: Duration = Duration::from_millis(400);
/// Cada cuánto se revisa si ha aparecido la carpeta de un agente nuevo.
const RECHECK: Duration = Duration::from_secs(20);

/// Fábrica de providers; se inyecta para poder probar el vigilante con carpetas de test.
/// Solo se usa para descubrir las carpetas que vigilar.
pub type Providers = Arc<dyn Fn() -> Vec<Box<dyn Provider>> + Send + Sync>;

/// Lanza el vigilante en su propio hilo; cada ráfaga de cambios pide un escaneo a `ingest`.
pub fn spawn(providers: Providers, ingest: IngestHandle) {
    std::thread::spawn(move || {
        if let Err(e) = run(providers, ingest, RECHECK) {
            tracing::error!("agentboard: el vigilante se detuvo: {e:#}");
        }
    });
}

fn existing_roots(providers: &Providers) -> Vec<PathBuf> {
    providers()
        .iter()
        .flat_map(|p| p.log_roots())
        .filter(|r| r.is_dir())
        .collect()
}

fn run(providers: Providers, ingest: IngestHandle, recheck: Duration) -> Result<()> {
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    })?;

    let mut watched: HashSet<PathBuf> = HashSet::new();
    // Devuelve si añadió alguna carpeta nueva a la vigilancia.
    let watch_new = |watcher: &mut notify::RecommendedWatcher, watched: &mut HashSet<PathBuf>| {
        let mut added = false;
        for root in existing_roots(&providers) {
            if watched.insert(root.clone()) {
                match watcher.watch(&root, RecursiveMode::Recursive) {
                    Ok(()) => added = true,
                    Err(e) => {
                        tracing::warn!(ruta = %root.display(), "agentboard: no se pudo vigilar {}: {e}", root.display());
                        watched.remove(&root);
                    }
                }
            }
        }
        added
    };
    watch_new(&mut watcher, &mut watched);

    loop {
        // Espera al primer evento (o revisa periódicamente si hay carpetas nuevas).
        match rx.recv_timeout(recheck) {
            Ok(()) => {
                // Agrupa la ráfaga: sigue drenando eventos hasta que haya DEBOUNCE de calma.
                let start = Instant::now();
                while rx.recv_timeout(DEBOUNCE).is_ok() && start.elapsed() < Duration::from_secs(5)
                {
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                // Una carpeta recién aparecida puede traer logs completos escritos antes de
                // vigilarla (esos eventos ya se perdieron): hay que escanearla ahora.
                if !watch_new(&mut watcher, &mut watched) {
                    continue;
                }
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }

        // Una carpeta aparecida durante la actividad se vigila ya; el escaneo pedido la incluye.
        watch_new(&mut watcher, &mut watched);
        ingest.request(Origen::Vigilante);
    }
}

// Este test de integración depende del backend de eventos del SO (inotify/FSEvents/
// ReadDirectoryChangesW) y de su latencia de entrega. En Linux (inotify) es rápido y
// determinista; en macOS y Windows la entrega en CI es lenta y variable, así que se
// ejecuta solo en Linux. La entrega multiplataforma es responsabilidad del crate `notify`.
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::db;
    use crate::ingest_thread::{IngestThread, Origen};
    use crate::providers::claude_code::ClaudeCode;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn escanea_carpetas_que_aparecen_despues_de_arrancar() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("projects");
        let target = db::Target::File(dir.path().join("cache.sqlite"));
        let writer = db::open_writer(&target).unwrap();
        let readers = db::ReadPool::open(&target, 1).unwrap();
        let root2 = root.clone();
        let providers: Providers = Arc::new(move || {
            vec![Box::new(ClaudeCode::with_roots(vec![root2.clone()])) as Box<dyn Provider>]
        });
        let ingest = IngestThread::spawn(writer, providers(), Default::default(), |_, _| {});
        let providers2 = providers.clone();
        std::thread::spawn(move || {
            let _ = run(providers2, ingest, Duration::from_millis(100));
        });
        // La carpeta aún no existe cuando arranca el vigilante.
        std::thread::sleep(Duration::from_millis(250));

        // El agente crea su carpeta y termina de escribir antes de que nadie la vigile.
        std::fs::create_dir_all(root.join("p")).unwrap();
        std::fs::write(
            root.join("p").join("s.jsonl"),
            r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-09-25T10:00:00Z","cwd":"/h/demo","message":{"model":"claude-sonnet-4-5","id":"m1","content":[],"usage":{"input_tokens":10,"output_tokens":5}}}"#
                .to_string()
                + "\n",
        )
        .unwrap();

        let count = || {
            readers
                .with(
                    |c| Ok(c.query_row("SELECT COUNT(*) FROM calls", [], |r| r.get::<_, i64>(0))?),
                )
                .unwrap()
        };
        let mut ok = false;
        for _ in 0..80 {
            std::thread::sleep(Duration::from_millis(100));
            if count() == 1 {
                ok = true;
                break;
            }
        }
        assert!(ok, "no escaneó la carpeta descubierta en el rechequeo");
    }

    #[test]
    fn relee_al_cambiar_un_log_sin_duplicar() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("projects");
        std::fs::create_dir_all(root.join("p")).unwrap();
        let file = root.join("p").join("s.jsonl");
        let line = |id: &str, ts: &str| {
            format!(
                r#"{{"type":"assistant","sessionId":"s1","timestamp":"{ts}","cwd":"/h/demo","message":{{"model":"claude-sonnet-4-5","id":"{id}","content":[],"usage":{{"input_tokens":10,"output_tokens":5}}}}}}"#
            ) + "\n"
        };
        std::fs::write(&file, line("m1", "2026-09-25T10:00:00Z")).unwrap();

        let target = db::Target::File(dir.path().join("cache.sqlite"));
        let writer = db::open_writer(&target).unwrap();
        let readers = db::ReadPool::open(&target, 1).unwrap();
        let root2 = root.clone();
        let providers: Providers = Arc::new(move || {
            vec![Box::new(ClaudeCode::with_roots(vec![root2.clone()])) as Box<dyn Provider>]
        });
        let calls = Arc::new(AtomicUsize::new(0));

        let calls2 = calls.clone();
        let ingest = IngestThread::spawn(
            writer,
            providers(),
            Default::default(),
            move |origen, stats| {
                if origen == Origen::Vigilante && stats.records > 0 {
                    calls2.fetch_add(1, Ordering::SeqCst);
                }
            },
        );
        let providers2 = providers.clone();
        std::thread::spawn(move || {
            let _ = run(providers2, ingest, Duration::from_millis(200));
        });

        let count = |sql: &str| {
            readers
                .with(|c| Ok(c.query_row(sql, [], |r| r.get::<_, i64>(0))?))
                .unwrap()
        };
        // Espera al escaneo inicial (que hace el propio vigilante al arrancar no; lo hace al primer evento).
        std::thread::sleep(Duration::from_millis(300));

        // Añade una llamada nueva: el vigilante debe detectarla y releer.
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        write!(f, "{}", line("m2", "2026-09-25T10:01:00Z")).unwrap();
        f.flush().unwrap();

        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(100));
            // El aviso a la UI llega justo después de soltar la base: se esperan ambas cosas.
            if count("SELECT COUNT(*) FROM calls") == 2 && calls.load(Ordering::SeqCst) >= 1 {
                break;
            }
        }
        assert_eq!(
            count("SELECT COUNT(*) FROM calls"),
            2,
            "el vigilante no releyó la llamada nueva"
        );
        assert!(calls.load(Ordering::SeqCst) >= 1, "no avisó a la UI");

        // Un cambio que no añade llamadas (tocar mtime reescribiendo lo mismo) no duplica.
        std::fs::write(
            &file,
            line("m1", "2026-09-25T10:00:00Z") + &line("m2", "2026-09-25T10:01:00Z"),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(
            count("SELECT COUNT(*) FROM calls"),
            2,
            "no debe duplicar al reescribir lo mismo"
        );
    }
}
