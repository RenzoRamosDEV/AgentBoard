//! Hilo de ingesta: el único escritor de la base. Es dueño de la conexión de escritura y atiende
//! por canal los escaneos (inicial y del vigilante, que se agrupan) y los cambios de precios, que
//! se aplican entre dos lotes aunque haya un escaneo en curso. Tras cada `COMMIT` sube la
//! generación de datos, que invalida la caché de resultados.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use crate::ingest::{self, BatchHooks, ScanStats};
use crate::providers::Provider;
use crate::settings::PriceOverride;
use anyhow::Result;
use rusqlite::Connection;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

/// Quién pide el escaneo: decide qué aviso recibe la interfaz al terminar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origen {
    EscaneoInicial,
    Vigilante,
}

/// Respuesta de un trabajo a quien lo pidió.
pub type Reply = Sender<Result<(), String>>;

/// Trabajos del escritor.
pub enum Job {
    Scan(Origen),
    AplicarPrecios(Vec<PriceOverride>, Reply),
}

/// Contador que sube tras cada transacción confirmada por el escritor (lote, precios…). Una
/// lectura hecha con la misma generación de principio a fin ve los mismos datos.
#[derive(Debug, Default)]
pub struct DataGeneration(AtomicU64);

impl DataGeneration {
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }

    fn bump(&self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }
}

/// Para pedir trabajos al hilo de ingesta desde cualquier hebra.
#[derive(Clone)]
pub struct IngestHandle(Sender<Job>);

impl IngestHandle {
    /// Encola un escaneo. Si el hilo ya terminó no hay nada que hacer.
    pub fn request(&self, origen: Origen) {
        let _ = self.0.send(Job::Scan(origen));
    }

    /// Aplica los precios del usuario y espera a que estén confirmados (bloquea: desde un
    /// comando, dentro de `spawn_blocking`).
    pub fn apply_prices(&self, overrides: Vec<PriceOverride>) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0
            .send(Job::AplicarPrecios(overrides, tx))
            .map_err(|_| "el hilo de ingesta no está en marcha".to_string())?;
        rx.recv()
            .map_err(|_| "el hilo de ingesta no respondió".to_string())?
    }
}

pub struct IngestThread;

impl IngestThread {
    /// Lanza el hilo con la conexión de escritura y un único conjunto de proveedores para toda la
    /// vida de la app (el estado entre líneas se conserva entre escaneos). `on_done` se llama
    /// tras cada escaneo.
    pub fn spawn(
        writer: Connection,
        providers: Vec<Box<dyn Provider>>,
        generation: Arc<DataGeneration>,
        on_done: impl FnMut(Origen, &ScanStats) + Send + 'static,
    ) -> IngestHandle {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || run(rx, writer, &providers, &generation, on_done));
        IngestHandle(tx)
    }
}

/// Lo que el escritor atiende entre lotes de un escaneo: precios en el momento; los escaneos
/// pedidos mientras tanto se juntan en uno al terminar.
struct Between<'a> {
    rx: &'a Receiver<Job>,
    generation: &'a DataGeneration,
    deferred: &'a mut VecDeque<Origen>,
}

impl BatchHooks for Between<'_> {
    fn committed(&mut self, conn: &mut Connection) -> Result<()> {
        self.generation.bump();
        self.waiting(conn)
    }

    fn waiting(&mut self, conn: &mut Connection) -> Result<()> {
        while let Ok(job) = self.rx.try_recv() {
            match job {
                Job::Scan(origen) => self.deferred.push_back(origen),
                Job::AplicarPrecios(overrides, reply) => {
                    apply_prices(conn, &overrides, self.generation, &reply)
                }
            }
        }
        Ok(())
    }
}

fn apply_prices(
    conn: &Connection,
    overrides: &[PriceOverride],
    generation: &DataGeneration,
    reply: &Reply,
) {
    let result = crate::pricing::apply_overrides(conn, overrides);
    if result.is_ok() {
        generation.bump();
    } else if let Err(e) = &result {
        tracing::error!("agentboard: no se pudieron aplicar los precios: {e:#}");
    }
    let _ = reply.send(result.map_err(|e| format!("{e:#}")));
}

/// Atiende trabajos hasta que se cierra el canal. Los escaneos acumulados se agrupan en uno
/// (es completo e idempotente); el escaneo inicial tiene prioridad para el aviso.
fn run(
    rx: Receiver<Job>,
    mut writer: Connection,
    providers: &[Box<dyn Provider>],
    generation: &DataGeneration,
    mut on_done: impl FnMut(Origen, &ScanStats),
) {
    let mut deferred: VecDeque<Origen> = VecDeque::new();
    loop {
        let first = match deferred.pop_front() {
            Some(origen) => Job::Scan(origen),
            None => match rx.recv() {
                Ok(job) => job,
                Err(_) => return,
            },
        };
        let mut origen = match first {
            Job::Scan(origen) => origen,
            Job::AplicarPrecios(overrides, reply) => {
                apply_prices(&writer, &overrides, generation, &reply);
                continue;
            }
        };
        // Agrupa los escaneos pendientes; los demás trabajos se atienden ya, antes de escanear.
        for o in deferred.drain(..) {
            if o == Origen::EscaneoInicial {
                origen = o;
            }
        }
        while let Ok(job) = rx.try_recv() {
            match job {
                Job::Scan(o) if o == Origen::EscaneoInicial => origen = o,
                Job::Scan(_) => {}
                Job::AplicarPrecios(overrides, reply) => {
                    apply_prices(&writer, &overrides, generation, &reply)
                }
            }
        }
        let mut hooks = Between {
            rx: &rx,
            generation,
            deferred: &mut deferred,
        };
        match ingest::scan_with_hooks(&mut writer, providers, &mut hooks) {
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
    use std::sync::Mutex;
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(20);

    /// Aviso de «empiezo a parsear» y permiso para seguir.
    type Gate = (Mutex<mpsc::Sender<String>>, Mutex<mpsc::Receiver<()>>);

    /// Proveedor de prueba: cada línea `<id>` es una sesión con una llamada. Si tiene `gate`,
    /// avisa al empezar a parsear las líneas que empiezan por `espera` y espera permiso.
    struct Fake {
        root: PathBuf,
        gate: Option<Gate>,
        delay: Duration,
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
            std::thread::sleep(self.delay);
            if let (Some((started, proceed)), true) = (&self.gate, line.starts_with("espera")) {
                started.lock().unwrap().send(line.to_string()).unwrap();
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
                    model: "claude-sonnet-4-5".into(),
                    input_tokens: 1_000_000,
                    ..Default::default()
                }),
            ])
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        target: db::Target,
    }

    fn fixture(files: &[(&str, &str)]) -> (Fixture, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("logs");
        std::fs::create_dir_all(&root).unwrap();
        for (name, body) in files {
            std::fs::write(root.join(name), body).unwrap();
        }
        let target = db::Target::File(dir.path().join("cache.sqlite"));
        let writer = db::open_writer(&target).unwrap();
        (
            Fixture {
                _dir: dir,
                root,
                target,
            },
            writer,
        )
    }

    fn fake(fx: &Fixture) -> Vec<Box<dyn Provider>> {
        vec![Box::new(Fake {
            root: fx.root.clone(),
            gate: None,
            delay: Duration::ZERO,
        })]
    }

    fn calls(fx: &Fixture) -> i64 {
        db::ReadPool::open(&fx.target, 1)
            .unwrap()
            .with(|c| Ok(c.query_row("SELECT COUNT(*) FROM calls", [], |r| r.get(0))?))
            .unwrap()
    }

    #[test]
    fn el_escaneo_inicial_avisa_con_sus_estadisticas() {
        let (fx, writer) = fixture(&[("a.log", "m1\nm2\n")]);
        let (tx, rx) = mpsc::channel();
        let handle = IngestThread::spawn(
            writer,
            fake(&fx),
            Default::default(),
            move |origen, stats| {
                tx.send((origen, stats.clone())).unwrap();
            },
        );
        handle.request(Origen::EscaneoInicial);
        let (origen, stats) = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(origen, Origen::EscaneoInicial);
        assert_eq!((stats.files, stats.records, stats.errors), (1, 4, 0));
        assert_eq!(calls(&fx), 2);
    }

    #[test]
    fn las_peticiones_acumuladas_se_agrupan_en_un_escaneo() {
        let (fx, writer) = fixture(&[("a.log", "m1\n")]);
        let (tx, rx) = mpsc::channel();
        tx.send(Job::Scan(Origen::Vigilante)).unwrap();
        tx.send(Job::Scan(Origen::EscaneoInicial)).unwrap();
        tx.send(Job::Scan(Origen::Vigilante)).unwrap();
        drop(tx);
        let mut done = Vec::new();
        run(
            rx,
            writer,
            &fake(&fx),
            &DataGeneration::default(),
            |origen, _| done.push(origen),
        );
        assert_eq!(
            done,
            vec![Origen::EscaneoInicial],
            "un escaneo, con el aviso inicial"
        );
    }

    #[test]
    fn los_precios_se_aplican_entre_lotes_de_un_escaneo_en_curso() {
        let (fx, writer) = fixture(&[("a.log", "m1\n"), ("b.log", "espera\n")]);
        let (started_tx, started_rx) = mpsc::channel();
        let (proceed_tx, proceed_rx) = mpsc::channel();
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(Fake {
            root: fx.root.clone(),
            gate: Some((Mutex::new(started_tx), Mutex::new(proceed_rx))),
            delay: Duration::ZERO,
        })];
        let (done_tx, done_rx) = mpsc::channel();
        let generation = Arc::new(DataGeneration::default());
        let handle = IngestThread::spawn(writer, providers, generation.clone(), move |_, s| {
            done_tx.send(s.clone()).unwrap();
        });
        handle.request(Origen::EscaneoInicial);
        assert_eq!(started_rx.recv_timeout(WAIT).unwrap(), "espera");
        // El escaneo está parado a mitad (b.log): el cambio de precios no espera a que acabe.
        let (reply_tx, reply_rx) = mpsc::channel();
        let h = handle.clone();
        std::thread::spawn(move || {
            reply_tx
                .send(h.apply_prices(vec![PriceOverride {
                    model: "claude-sonnet-4-5".into(),
                    input: 1.0,
                    output: 0.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                    cache_write_1h: 0.0,
                }]))
                .unwrap()
        });
        assert_eq!(reply_rx.recv_timeout(WAIT).unwrap(), Ok(()));
        let cost: f64 = db::ReadPool::open(&fx.target, 1)
            .unwrap()
            .with(|c| Ok(c.query_row("SELECT SUM(cost_usd) FROM calls", [], |r| r.get(0))?))
            .unwrap();
        assert!(
            (cost - 1.0).abs() < 1e-9,
            "m1 ya con el precio nuevo: {cost}"
        );
        proceed_tx.send(()).unwrap();
        let stats = done_rx.recv_timeout(WAIT).unwrap();
        assert_eq!((stats.files, stats.errors), (2, 0));
        assert_eq!(calls(&fx), 2, "sin perder ni duplicar");
    }

    #[test]
    fn la_generacion_sube_tras_cada_commit_y_no_antes() {
        let files: Vec<(String, String)> = (0..5)
            .map(|i| (format!("{i}.log"), format!("m{i}\n")))
            .collect();
        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let (fx, mut writer) = fixture(&refs);
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(Fake {
            root: fx.root.clone(),
            gate: None,
            delay: Duration::from_millis(30),
        })];
        let generation = DataGeneration::default();
        let (dummy_tx, rx) = mpsc::channel();
        let mut deferred = VecDeque::new();
        struct Check<'a> {
            inner: Between<'a>,
            fx: &'a Fixture,
            seen: Vec<(u64, i64)>,
        }
        impl BatchHooks for Check<'_> {
            fn committed(&mut self, conn: &mut Connection) -> Result<()> {
                let before = self.inner.generation.get();
                let visible = calls(self.fx);
                self.inner.committed(conn)?;
                self.seen
                    .push((self.inner.generation.get() - before, visible));
                Ok(())
            }
            fn waiting(&mut self, conn: &mut Connection) -> Result<()> {
                self.inner.waiting(conn)
            }
        }
        let mut check = Check {
            inner: Between {
                rx: &rx,
                generation: &generation,
                deferred: &mut deferred,
            },
            fx: &fx,
            seen: Vec::new(),
        };
        crate::ingest::scan_with_hooks(&mut writer, &providers, &mut check).unwrap();
        drop(dummy_tx);
        let seen = check.seen;
        assert!(!seen.is_empty());
        // Cada aviso llega con el lote ya visible para un lector, y sube la generación en 1.
        let mut last = 0;
        for (bumped, visible) in &seen {
            assert_eq!(*bumped, 1);
            assert!(*visible > last, "{seen:?}");
            last = *visible;
        }
        assert_eq!(generation.get(), seen.len() as u64);
        assert_eq!(last, 5);
    }

    #[test]
    fn un_trabajo_fallido_no_detiene_el_hilo() {
        let (fx, writer) = fixture(&[("a.log", "m1\n")]);
        writer
            .execute_batch(
                "CREATE TRIGGER sin_precios BEFORE INSERT ON prices BEGIN SELECT RAISE(ABORT, 'no'); END;",
            )
            .unwrap();
        let (tx, rx) = mpsc::channel();
        let handle =
            IngestThread::spawn(writer, fake(&fx), Default::default(), move |origen, _| {
                tx.send(origen).unwrap();
            });
        let nuevo = PriceOverride {
            model: "modelo-nuevo".into(),
            input: 1.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            cache_write_1h: 0.0,
        };
        assert!(
            handle.apply_prices(vec![nuevo]).is_err(),
            "el precio nuevo no se puede escribir"
        );
        handle.request(Origen::Vigilante);
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), Origen::Vigilante);
        assert_eq!(calls(&fx), 1);
    }
}
