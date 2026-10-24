//! Hilo de ingesta: el único que escanea los logs (escaneo inicial y relecturas del vigilante).
//! Recibe peticiones por canal y agrupa las que se acumulan en un único escaneo.

use crate::ingest::{self, ScanStats};
use crate::providers::Provider;
use rusqlite::Connection;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Quién pide el escaneo: decide qué aviso recibe la interfaz al terminar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origen {
    EscaneoInicial,
    Vigilante,
}

/// Para pedir escaneos al hilo de ingesta desde cualquier hebra.
#[derive(Clone)]
pub struct IngestHandle(Sender<Origen>);

impl IngestHandle {
    /// Encola un escaneo. Si el hilo ya terminó no hay nada que hacer.
    pub fn request(&self, origen: Origen) {
        let _ = self.0.send(origen);
    }
}

pub struct IngestThread;

impl IngestThread {
    /// Lanza el hilo con un único conjunto de proveedores para toda la vida de la app (el
    /// estado entre líneas se conserva entre escaneos). `on_done` se llama tras cada escaneo.
    pub fn spawn(
        db: Arc<Mutex<Connection>>,
        providers: Vec<Box<dyn Provider>>,
        on_done: impl FnMut(Origen, &ScanStats) + Send + 'static,
    ) -> IngestHandle {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || run(rx, &db, &providers, on_done));
        IngestHandle(tx)
    }
}

/// Atiende peticiones hasta que se cierra el canal. Las acumuladas se agrupan en un único
/// escaneo (es completo e idempotente); el escaneo inicial tiene prioridad para el aviso.
fn run(
    rx: Receiver<Origen>,
    db: &Mutex<Connection>,
    providers: &[Box<dyn Provider>],
    mut on_done: impl FnMut(Origen, &ScanStats),
) {
    while let Ok(first) = rx.recv() {
        let origen = rx.try_iter().fold(
            first,
            |acc, o| {
                if o == Origen::EscaneoInicial {
                    o
                } else {
                    acc
                }
            },
        );
        match ingest::scan_shared(db, providers) {
            Ok(stats) => on_done(origen, &stats),
            Err(e) => tracing::error!(?origen, "agentboard: falló el escaneo: {e:#}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::providers::{CallRec, Provider, Record, SessionRec};
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(20);

    /// Proveedor de prueba: cada línea `<id>` es una sesión con una llamada. Si tiene `gate`,
    /// avisa al empezar a parsear y espera permiso para seguir.
    struct Fake {
        root: PathBuf,
        gate: Option<(Mutex<mpsc::Sender<()>>, Mutex<mpsc::Receiver<()>>)>,
    }

    impl Provider for Fake {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn name(&self) -> &'static str {
            "Fake"
        }
        fn log_roots(&self) -> Vec<PathBuf> {
            vec![self.root.clone()]
        }
        fn matches(&self, path: &Path) -> bool {
            path.extension().is_some_and(|e| e == "log")
        }
        fn parse_line(&self, _path: &Path, line: &str) -> anyhow::Result<Vec<Record>> {
            if let Some((started, proceed)) = &self.gate {
                started.lock().unwrap().send(()).unwrap();
                proceed.lock().unwrap().recv().unwrap();
            }
            Ok(vec![
                Record::Session(SessionRec {
                    id: format!("s-{line}"),
                    cwd: None,
                    git_branch: None,
                    ts: 1,
                    is_subagent: false,
                }),
                Record::Call(CallRec {
                    message_id: line.to_string(),
                    session_id: format!("s-{line}"),
                    ts: 1,
                    model: "m".into(),
                    ..Default::default()
                }),
            ])
        }
    }

    fn fixture(lines: &str) -> (tempfile::TempDir, Arc<Mutex<rusqlite::Connection>>) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.log"), lines).unwrap();
        (dir, Arc::new(Mutex::new(db::open_in_memory().unwrap())))
    }

    fn fake(dir: &tempfile::TempDir) -> Vec<Box<dyn Provider>> {
        vec![Box::new(Fake {
            root: dir.path().to_path_buf(),
            gate: None,
        })]
    }

    fn calls(db: &Mutex<rusqlite::Connection>) -> i64 {
        db.lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM calls", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn el_escaneo_inicial_avisa_con_sus_estadisticas() {
        let (dir, db) = fixture("m1\nm2\n");
        let (tx, rx) = mpsc::channel();
        let handle = IngestThread::spawn(db.clone(), fake(&dir), move |origen, stats| {
            tx.send((origen, stats.clone())).unwrap();
        });
        handle.request(Origen::EscaneoInicial);
        let (origen, stats) = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(origen, Origen::EscaneoInicial);
        assert_eq!((stats.files, stats.records, stats.errors), (1, 4, 0));
        assert_eq!(calls(&db), 2);
    }

    #[test]
    fn las_peticiones_acumuladas_se_agrupan_en_un_escaneo() {
        let (dir, db) = fixture("m1\n");
        let (tx, rx) = mpsc::channel();
        tx.send(Origen::Vigilante).unwrap();
        tx.send(Origen::EscaneoInicial).unwrap();
        tx.send(Origen::Vigilante).unwrap();
        drop(tx);
        let mut done = Vec::new();
        run(rx, &db, &fake(&dir), |origen, _| done.push(origen));
        assert_eq!(
            done,
            vec![Origen::EscaneoInicial],
            "un escaneo, con el aviso inicial"
        );
    }

    #[test]
    fn la_base_queda_libre_mientras_se_parsea() {
        let (dir, db) = fixture("m1\n");
        let (started_tx, started_rx) = mpsc::channel();
        let (proceed_tx, proceed_rx) = mpsc::channel();
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(Fake {
            root: dir.path().to_path_buf(),
            gate: Some((Mutex::new(started_tx), Mutex::new(proceed_rx))),
        })];
        let (done_tx, done_rx) = mpsc::channel();
        let handle = IngestThread::spawn(db.clone(), providers, move |_, _| {
            done_tx.send(()).unwrap();
        });
        handle.request(Origen::EscaneoInicial);
        started_rx.recv_timeout(WAIT).unwrap();
        let conn = db
            .try_lock()
            .expect("la ingesta no retiene la base mientras parsea");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM agents", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "el agente ya está registrado");
        drop(conn);
        proceed_tx.send(()).unwrap();
        done_rx.recv_timeout(WAIT).unwrap();
        assert_eq!(calls(&db), 1);
    }

    #[test]
    fn un_escaneo_fallido_no_detiene_el_hilo() {
        let (dir, db) = fixture("m1\n");
        let poison = db.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poison.lock().unwrap();
            panic!("envenena la base");
        })
        .join();
        let (tx, rx) = mpsc::channel();
        let handle = IngestThread::spawn(db.clone(), fake(&dir), move |origen, _| {
            tx.send(origen).unwrap();
        });
        handle.request(Origen::EscaneoInicial);
        assert!(
            rx.recv_timeout(Duration::from_millis(500)).is_err(),
            "falló: sin aviso"
        );
        db.clear_poison();
        handle.request(Origen::Vigilante);
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), Origen::Vigilante);
        assert_eq!(calls(&db), 1);
    }
}
