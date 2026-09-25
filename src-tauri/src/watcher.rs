//! Vigilante de archivos: observa las carpetas de logs de los agentes y, al cambiar algo,
//! relee de forma incremental (los offsets de `file_state` evitan duplicar) y avisa a la UI.

use crate::{ingest, providers};
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

/// Lanza el vigilante en su propio hilo. `on_change` se llama tras cada relectura con cambios.
pub fn spawn(db: Arc<Mutex<Connection>>, on_change: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        if let Err(e) = run(db, on_change) {
            eprintln!("agentboard: el vigilante se detuvo: {e:#}");
        }
    });
}

/// Carpetas de logs que existen ahora mismo (las que hay que observar).
fn existing_roots() -> Vec<PathBuf> {
    providers::all().iter().flat_map(|p| p.log_roots()).filter(|r| r.is_dir()).collect()
}

fn run(db: Arc<Mutex<Connection>>, on_change: impl Fn()) -> Result<()> {
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    })?;

    let mut watched: HashSet<PathBuf> = HashSet::new();
    let mut watch_new = |watcher: &mut notify::RecommendedWatcher, watched: &mut HashSet<PathBuf>| {
        for root in existing_roots() {
            if watched.insert(root.clone()) {
                if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
                    eprintln!("agentboard: no se pudo vigilar {}: {e}", root.display());
                    watched.remove(&root);
                }
            }
        }
    };
    watch_new(&mut watcher, &mut watched);

    let mut last_recheck = Instant::now();
    loop {
        // Espera al primer evento (o revisa periódicamente si hay carpetas nuevas).
        match rx.recv_timeout(RECHECK) {
            Ok(()) => {}
            Err(RecvTimeoutError::Timeout) => {
                if last_recheck.elapsed() >= RECHECK {
                    watch_new(&mut watcher, &mut watched);
                    last_recheck = Instant::now();
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        // Agrupa la ráfaga: sigue drenando eventos hasta que haya DEBOUNCE de calma.
        while rx.recv_timeout(DEBOUNCE).is_ok() {}

        let providers = providers::all();
        let scanned = match db.lock() {
            Ok(mut conn) => ingest::scan_all(&mut conn, &providers).map(|s| s.records).unwrap_or(0),
            Err(_) => 0,
        };
        // Puede haber aparecido una carpeta nueva durante la actividad.
        watch_new(&mut watcher, &mut watched);
        last_recheck = Instant::now();
        if scanned > 0 {
            on_change();
        }
    }
}
