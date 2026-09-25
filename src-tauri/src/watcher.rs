//! Vigilante de archivos: observa las carpetas de logs de los agentes y, al cambiar algo,
//! relee de forma incremental (los offsets de `file_state` evitan duplicar) y avisa a la UI.

use crate::ingest;
use crate::providers::Provider;
use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Espera tras el primer cambio antes de releer, para agrupar ráfagas de escrituras.
const DEBOUNCE: Duration = Duration::from_millis(400);
/// Cada cuánto se revisa si ha aparecido la carpeta de un agente nuevo.
const RECHECK: Duration = Duration::from_secs(20);

/// Fábrica de providers; se inyecta para poder probar el vigilante con carpetas de test.
pub type Providers = Arc<dyn Fn() -> Vec<Box<dyn Provider>> + Send + Sync>;

/// Lanza el vigilante en su propio hilo. `on_change` se llama tras cada relectura con cambios.
pub fn spawn(
    db: Arc<Mutex<Connection>>,
    providers: Providers,
    on_change: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        if let Err(e) = run(db, providers, on_change, RECHECK) {
            eprintln!("agentboard: el vigilante se detuvo: {e:#}");
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

fn run(
    db: Arc<Mutex<Connection>>,
    providers: Providers,
    on_change: impl Fn(),
    recheck: Duration,
) -> Result<()> {
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    })?;

    let mut watched: HashSet<PathBuf> = HashSet::new();
    let watch_new = |watcher: &mut notify::RecommendedWatcher, watched: &mut HashSet<PathBuf>| {
        for root in existing_roots(&providers) {
            if watched.insert(root.clone()) {
                if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
                    eprintln!("agentboard: no se pudo vigilar {}: {e}", root.display());
                    watched.remove(&root);
                }
            }
        }
    };
    watch_new(&mut watcher, &mut watched);

    loop {
        // Espera al primer evento (o revisa periódicamente si hay carpetas nuevas).
        match rx.recv_timeout(recheck) {
            Ok(()) => {}
            Err(RecvTimeoutError::Timeout) => {
                watch_new(&mut watcher, &mut watched);
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        // Agrupa la ráfaga: sigue drenando eventos hasta que haya DEBOUNCE de calma.
        let start = Instant::now();
        while rx.recv_timeout(DEBOUNCE).is_ok() && start.elapsed() < Duration::from_secs(5) {}

        let scanned = match db.lock() {
            Ok(mut conn) => ingest::scan_all(&mut conn, &providers())
                .map(|s| s.records)
                .unwrap_or(0),
            Err(_) => 0,
        };
        // Puede haber aparecido una carpeta nueva durante la actividad.
        watch_new(&mut watcher, &mut watched);
        if scanned > 0 {
            on_change();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::providers::claude_code::ClaudeCode;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

        let db = Arc::new(Mutex::new(db::open_in_memory().unwrap()));
        let root2 = root.clone();
        let providers: Providers = Arc::new(move || {
            vec![Box::new(ClaudeCode::with_roots(vec![root2.clone()])) as Box<dyn Provider>]
        });
        let calls = Arc::new(AtomicUsize::new(0));

        let db2 = db.clone();
        let providers2 = providers.clone();
        let calls2 = calls.clone();
        std::thread::spawn(move || {
            let _ = run(
                db2,
                providers2,
                move || {
                    calls2.fetch_add(1, Ordering::SeqCst);
                },
                Duration::from_millis(200),
            );
        });

        let count = |sql: &str| {
            db.lock()
                .unwrap()
                .query_row(sql, [], |r| r.get::<_, i64>(0))
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

        let mut ok = false;
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(100));
            if count("SELECT COUNT(*) FROM calls") == 2 {
                ok = true;
                break;
            }
        }
        assert!(ok, "el vigilante no releyó la llamada nueva");
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
