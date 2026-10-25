//! Hilo de ingesta: el único escritor de la base. Es dueño de la conexión de escritura y atiende
//! por canal los escaneos (inicial y del vigilante, que se agrupan), los cambios de precios, que
//! se aplican entre dos lotes aunque haya un escaneo en curso, y las reconstrucciones de la caché
//! (pedidas por el usuario o tras detectar una base dañada). Tras cada `COMMIT` sube la
//! generación de datos, que invalida la caché de resultados.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use crate::db::{CacheMode, ReadPool, SharedReaders, Storage};
use crate::derived::Touched;
use crate::ingest::{self, BatchHooks, ScanStats};
use crate::providers::Provider;
use crate::settings::PriceOverride;
use anyhow::Result;
use rusqlite::Connection;
use std::collections::{BTreeSet, VecDeque};
use std::path::PathBuf;
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
    /// Relectura dirigida de los archivos que avisó el vigilante.
    Rescan(BTreeSet<PathBuf>),
    AplicarPrecios(Vec<PriceOverride>, Reply),
    /// Vacía la caché (en el modo pedido o, sin él, el actual) y vuelve a leer todos los logs.
    Reconstruir(Option<CacheMode>, Option<Reply>),
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

    #[cfg(test)]
    pub fn bump_for_tests(&self) {
        self.bump();
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

    /// Encola la relectura dirigida de `paths` (del vigilante).
    pub fn rescan(&self, paths: BTreeSet<PathBuf>) {
        let _ = self.0.send(Job::Rescan(paths));
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

    /// Reconstruye la caché en `mode` (o en el actual) y espera a que esté vacía y lista; el
    /// escaneo completo sigue después y avisa con `ingest://done`.
    pub fn rebuild(&self, mode: Option<CacheMode>) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0
            .send(Job::Reconstruir(mode, Some(tx)))
            .map_err(|_| "el hilo de ingesta no está en marcha".to_string())?;
        rx.recv()
            .map_err(|_| "el hilo de ingesta no respondió".to_string())?
    }

    /// Si `e` es de una base dañada, pide reconstruir la caché (sin esperar).
    pub fn report(&self, e: &anyhow::Error) {
        if crate::db::is_corruption(e) {
            tracing::warn!(
                "agentboard: la caché está dañada; se reconstruye desde los logs: {e:#}"
            );
            let _ = self.0.send(Job::Reconstruir(None, None));
        }
    }
}

/// Lo que posee el escritor: la base, el conjunto de lectores que publica y de dónde saca los
/// precios del usuario al reconstruir.
struct WriterState {
    storage: Storage,
    readers: SharedReaders,
    prices: Box<dyn Fn() -> Vec<PriceOverride> + Send>,
    on_change: Box<OnChange>,
}

/// Aviso tras cada transacción confirmada con cambios: generación nueva y lo que cambió.
type OnChange = dyn FnMut(u64, &Touched) + Send;

impl WriterState {
    /// Sube la generación y, si cambió algo, avisa.
    fn changed(&mut self, generation: &DataGeneration, touched: &Touched) {
        generation.bump();
        if !touched.is_empty() {
            (self.on_change)(generation.get(), touched);
        }
    }
}

pub struct IngestThread;

impl IngestThread {
    /// Lanza el hilo con la conexión de escritura y un único conjunto de proveedores para toda la
    /// vida de la app (el estado entre líneas se conserva entre escaneos). `on_done` se llama
    /// tras cada escaneo.
    pub fn spawn(
        storage: Storage,
        readers: SharedReaders,
        prices: impl Fn() -> Vec<PriceOverride> + Send + 'static,
        providers: Vec<Box<dyn Provider>>,
        generation: Arc<DataGeneration>,
        on_done: impl FnMut(Origen, &ScanStats) + Send + 'static,
    ) -> IngestHandle {
        Self::spawn_with_changes(
            storage,
            readers,
            prices,
            providers,
            generation,
            on_done,
            |_, _| {},
        )
    }

    /// `spawn` avisando a `on_change` tras cada transacción confirmada con cambios, con la
    /// generación nueva y lo que cambió.
    pub fn spawn_with_changes(
        storage: Storage,
        readers: SharedReaders,
        prices: impl Fn() -> Vec<PriceOverride> + Send + 'static,
        providers: Vec<Box<dyn Provider>>,
        generation: Arc<DataGeneration>,
        on_done: impl FnMut(Origen, &ScanStats) + Send + 'static,
        on_change: impl FnMut(u64, &Touched) + Send + 'static,
    ) -> IngestHandle {
        let (tx, rx) = mpsc::channel();
        let state = WriterState {
            storage,
            readers,
            prices: Box::new(prices),
            on_change: Box::new(on_change),
        };
        std::thread::spawn(move || run(rx, state, &providers, &generation, on_done));
        IngestHandle(tx)
    }
}

/// Lo que el escritor atiende entre lotes de un escaneo: precios en el momento; los escaneos y
/// reconstrucciones pedidos mientras tanto, al terminar.
struct Between<'a> {
    rx: &'a Receiver<Job>,
    generation: &'a DataGeneration,
    deferred: &'a mut VecDeque<Job>,
    on_change: &'a mut dyn FnMut(u64, &Touched),
}

impl Between<'_> {
    fn changed(&mut self, touched: &Touched) {
        self.generation.bump();
        if !touched.is_empty() {
            (self.on_change)(self.generation.get(), touched);
        }
    }
}

impl BatchHooks for Between<'_> {
    fn committed(&mut self, conn: &mut Connection, touched: &Touched) -> Result<()> {
        self.changed(touched);
        self.waiting(conn)
    }

    fn waiting(&mut self, conn: &mut Connection) -> Result<()> {
        while let Ok(job) = self.rx.try_recv() {
            match job {
                Job::AplicarPrecios(overrides, reply) => {
                    if apply_prices(conn, &overrides, &reply) {
                        self.changed(&Touched::everything());
                    }
                }
                other => self.deferred.push_back(other),
            }
        }
        Ok(())
    }
}

/// Aplica los precios y responde; devuelve si se confirmaron.
fn apply_prices(conn: &Connection, overrides: &[PriceOverride], reply: &Reply) -> bool {
    let result = crate::pricing::apply_overrides(conn, overrides);
    if let Err(e) = &result {
        tracing::error!("agentboard: no se pudieron aplicar los precios: {e:#}");
    }
    let ok = result.is_ok();
    let _ = reply.send(result.map_err(|e| format!("{e:#}")));
    ok
}

/// Vacía la caché: crea la base nueva (borrando antes los archivos si había caché en disco),
/// aplica los precios, publica sus lectores (las consultas en curso terminan con los anteriores)
/// y suelta la anterior. El escaneo completo lo hace después el llamador.
fn rebuild(
    state: &mut WriterState,
    mode: Option<CacheMode>,
    generation: &DataGeneration,
) -> Result<()> {
    let mode = mode.unwrap_or_else(|| state.storage.mode.clone());
    if let CacheMode::Disco(old) = &state.storage.mode {
        crate::db::delete_cache_files(old);
    }
    let storage = Storage::open(mode)?;
    crate::pricing::apply_overrides(&storage.writer, &(state.prices)())?;
    let pool = Arc::new(storage.readers(ReadPool::SIZE)?);
    match state.readers.write() {
        Ok(mut current) => *current = pool,
        Err(poisoned) => *poisoned.into_inner() = pool,
    }
    state.storage = storage;
    state.changed(generation, &Touched::everything());
    Ok(())
}

/// Checkpoint `TRUNCATE` del WAL si supera su límite; si un lector lo impide, se reintenta tras
/// el siguiente trabajo.
fn checkpoint(storage: &Storage) {
    match crate::db::checkpoint_if_large(&storage.writer, &storage.target) {
        Ok(crate::db::Checkpoint::Busy) => {
            tracing::debug!("agentboard: checkpoint del WAL aplazado por un lector")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("agentboard: falló el checkpoint del WAL: {e:#}"),
    }
}

/// Lectura pendiente: un escaneo completo o la relectura dirigida de unas rutas.
enum Pending {
    Full(Origen),
    Paths(BTreeSet<PathBuf>),
}

impl Pending {
    /// Une otra petición: las rutas se suman, un escaneo completo absorbe las relecturas y el
    /// escaneo inicial tiene prioridad para el aviso.
    fn absorb(self, other: Pending) -> Pending {
        match (self, other) {
            (Pending::Paths(mut a), Pending::Paths(b)) => {
                a.extend(b);
                Pending::Paths(a)
            }
            (Pending::Full(a), Pending::Full(b)) => Pending::Full(
                if a == Origen::EscaneoInicial || b == Origen::EscaneoInicial {
                    Origen::EscaneoInicial
                } else {
                    a
                },
            ),
            (Pending::Full(o), Pending::Paths(_)) | (Pending::Paths(_), Pending::Full(o)) => {
                Pending::Full(o)
            }
        }
    }

    fn origen(&self) -> Origen {
        match self {
            Pending::Full(o) => *o,
            Pending::Paths(_) => Origen::Vigilante,
        }
    }

    fn into_job(self) -> Job {
        match self {
            Pending::Full(o) => Job::Scan(o),
            Pending::Paths(p) => Job::Rescan(p),
        }
    }
}

/// Atiende trabajos hasta que se cierra el canal. Los escaneos y relecturas acumulados se agrupan
/// en una pasada (son idempotentes); el escaneo inicial tiene prioridad para el aviso.
fn run(
    rx: Receiver<Job>,
    mut state: WriterState,
    providers: &[Box<dyn Provider>],
    generation: &DataGeneration,
    mut on_done: impl FnMut(Origen, &ScanStats),
) {
    let mut deferred: VecDeque<Job> = VecDeque::new();
    loop {
        let job = match deferred.pop_front() {
            Some(job) => job,
            None => match rx.recv() {
                Ok(job) => job,
                Err(_) => return,
            },
        };
        let mut pending = match job {
            Job::Scan(origen) => Pending::Full(origen),
            Job::Rescan(paths) => Pending::Paths(paths),
            Job::AplicarPrecios(overrides, reply) => {
                if apply_prices(&state.storage.writer, &overrides, &reply) {
                    state.changed(generation, &Touched::everything());
                }
                continue;
            }
            Job::Reconstruir(mode, reply) => {
                let result = rebuild(&mut state, mode, generation);
                if let Err(e) = &result {
                    tracing::error!("agentboard: no se pudo reconstruir la caché: {e:#}");
                }
                let ok = result.is_ok();
                if let Some(reply) = reply {
                    let _ = reply.send(result.map_err(|e| format!("{e:#}")));
                }
                if ok {
                    deferred.push_front(Job::Scan(Origen::EscaneoInicial));
                }
                continue;
            }
        };
        // Agrupa los escaneos y relecturas pendientes; precios y reconstrucciones van antes.
        let mut later = VecDeque::new();
        for job in deferred.drain(..).chain(rx.try_iter()) {
            match job {
                Job::Scan(o) => pending = pending.absorb(Pending::Full(o)),
                Job::Rescan(paths) => pending = pending.absorb(Pending::Paths(paths)),
                Job::AplicarPrecios(overrides, reply) => {
                    if apply_prices(&state.storage.writer, &overrides, &reply) {
                        state.changed(generation, &Touched::everything());
                    }
                }
                other => later.push_back(other),
            }
        }
        if !later.is_empty() {
            // Una reconstrucción pendiente va primero; la lectura pedida se hace después.
            deferred = later;
            deferred.push_back(pending.into_job());
            continue;
        }
        let origen = pending.origen();
        let mut hooks = Between {
            rx: &rx,
            generation,
            deferred: &mut deferred,
            on_change: &mut *state.on_change,
        };
        let conn = &mut state.storage.writer;
        let result = match &pending {
            Pending::Full(_) => ingest::scan_with_hooks(conn, providers, &mut hooks),
            Pending::Paths(paths) => {
                ingest::scan_paths_with_hooks(conn, providers, paths, &mut hooks)
            }
        };
        match result {
            Ok(stats) => {
                on_done(origen, &stats);
                // Con la cola vacía el escritor queda libre: momento de reiniciar el WAL.
                match rx.try_recv() {
                    Ok(job) => deferred.push_back(job),
                    Err(_) if deferred.is_empty() => checkpoint(&state.storage),
                    Err(_) => {}
                }
            }
            Err(e) => {
                tracing::error!(?origen, "agentboard: falló el escaneo: {e:#}");
                if crate::db::is_corruption(&e) {
                    tracing::warn!(
                        "agentboard: la caché está dañada; se reconstruye desde los logs"
                    );
                    deferred.push_front(Job::Reconstruir(None, None));
                }
            }
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

    fn fixture(files: &[(&str, &str)]) -> (Fixture, Storage) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("logs");
        std::fs::create_dir_all(&root).unwrap();
        for (name, body) in files {
            std::fs::write(root.join(name), body).unwrap();
        }
        let storage = Storage::open(CacheMode::Disco(dir.path().join("cache.sqlite"))).unwrap();
        (
            Fixture {
                _dir: dir,
                root,
                target: storage.target.clone(),
            },
            storage,
        )
    }

    fn readers_of(storage: &Storage) -> SharedReaders {
        Arc::new(std::sync::RwLock::new(Arc::new(
            storage.readers(1).unwrap(),
        )))
    }

    fn spawn(
        storage: Storage,
        providers: Vec<Box<dyn Provider>>,
        generation: Arc<DataGeneration>,
        on_done: impl FnMut(Origen, &ScanStats) + Send + 'static,
    ) -> IngestHandle {
        let readers = readers_of(&storage);
        IngestThread::spawn(storage, readers, Vec::new, providers, generation, on_done)
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
        let handle = spawn(
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
        let readers = readers_of(&writer);
        let writer = WriterState {
            storage: writer,
            readers,
            prices: Box::new(Vec::new),
            on_change: Box::new(|_, _| {}),
        };
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

    fn run_jobs(fx: &Fixture, writer: Storage, jobs: Vec<Job>) -> Vec<(Origen, ScanStats)> {
        let (tx, rx) = mpsc::channel();
        for job in jobs {
            tx.send(job).unwrap();
        }
        drop(tx);
        let mut done = Vec::new();
        let readers = readers_of(&writer);
        let writer = WriterState {
            storage: writer,
            readers,
            prices: Box::new(Vec::new),
            on_change: Box::new(|_, _| {}),
        };
        run(
            rx,
            writer,
            &fake(fx),
            &DataGeneration::default(),
            |origen, stats| done.push((origen, stats.clone())),
        );
        done
    }

    fn rescan(paths: &[&Path]) -> Job {
        Job::Rescan(paths.iter().map(|p| p.to_path_buf()).collect())
    }

    #[test]
    fn las_relecturas_pendientes_se_unen_en_una_pasada_dirigida() {
        let (fx, writer) = fixture(&[("a.log", "m1\n"), ("b.log", "m2\n"), ("c.log", "m3\n")]);
        let (a, b) = (fx.root.join("a.log"), fx.root.join("b.log"));
        let done = run_jobs(&fx, writer, vec![rescan(&[&a]), rescan(&[&b, &a])]);
        assert_eq!(done.len(), 1, "una pasada");
        let (origen, stats) = &done[0];
        assert_eq!(*origen, Origen::Vigilante);
        assert!(!stats.full, "relectura dirigida");
        assert_eq!(stats.files, 2, "a y b, sin c");
        assert_eq!(calls(&fx), 2);
    }

    #[test]
    fn un_escaneo_completo_pendiente_absorbe_las_relecturas() {
        let (fx, writer) = fixture(&[("a.log", "m1\n"), ("b.log", "m2\n"), ("c.log", "m3\n")]);
        let (a, b) = (fx.root.join("a.log"), fx.root.join("b.log"));
        let done = run_jobs(
            &fx,
            writer,
            vec![rescan(&[&a]), Job::Scan(Origen::Vigilante), rescan(&[&b])],
        );
        assert_eq!(done.len(), 1, "una pasada");
        assert!(done[0].1.full);
        assert_eq!(done[0].1.files, 3);
        assert_eq!(calls(&fx), 3);
    }

    #[test]
    fn un_commit_sin_cambios_no_avisa_y_los_precios_avisan_de_todo() {
        let (fx, storage) = fixture(&[("a.log", "m1\n")]);
        let readers = readers_of(&storage);
        let (done_tx, done_rx) = mpsc::channel();
        let (change_tx, change_rx) = mpsc::channel();
        let handle = IngestThread::spawn_with_changes(
            storage,
            readers,
            Vec::new,
            fake(&fx),
            Default::default(),
            move |_, s| done_tx.send(s.clone()).unwrap(),
            move |generation, touched| change_tx.send((generation, touched.clone())).unwrap(),
        );
        handle.request(Origen::EscaneoInicial);
        done_rx.recv_timeout(WAIT).unwrap();
        let (generation, touched) = change_rx.recv_timeout(WAIT).unwrap();
        assert!(generation >= 1);
        assert_eq!(touched.agents, ["fake".to_string()].into());
        assert!(!touched.all);
        // Media línea: se lee el archivo y se confirma su estado, pero no hay datos nuevos.
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(fx.root.join("a.log"))
            .unwrap()
            .write_all(b"m2")
            .unwrap();
        handle.rescan([fx.root.join("a.log")].into());
        let stats = done_rx.recv_timeout(WAIT).unwrap();
        assert_eq!(stats.records, 0);
        assert!(change_rx.try_recv().is_err(), "sin cambios no se avisa");
        handle.apply_prices(Vec::new()).unwrap();
        let (_, touched) = change_rx.recv_timeout(WAIT).unwrap();
        assert!(touched.all, "los precios pueden cambiar todo");
    }

    #[test]
    fn el_wal_grande_se_trunca_cuando_el_escritor_queda_libre_y_no_hay_lectores() {
        let (fx, storage) = fixture(&[("a.log", "m1\n")]);
        let path = match &storage.mode {
            CacheMode::Disco(p) => p.clone(),
            CacheMode::Memoria => unreachable!(),
        };
        let wal = || {
            std::fs::metadata(format!("{}-wal", path.display()))
                .map(|m| m.len())
                .unwrap_or(0)
        };
        const LIMIT: u64 = 8 * 1024 * 1024;
        // Un lector a mitad de su lectura mientras llega un lote de 10 MiB: el WAL no se reinicia.
        let (hold_tx, hold_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let target = fx.target.clone();
        let holder = std::thread::spawn(move || {
            db::ReadPool::open(&target, 1)
                .unwrap()
                .with(|c| {
                    c.execute_batch("BEGIN")?;
                    let _: i64 = c.query_row("SELECT COUNT(*) FROM calls", [], |r| r.get(0))?;
                    ready_tx.send(()).ok();
                    hold_rx.recv().ok();
                    c.execute_batch("COMMIT")?;
                    Ok(())
                })
                .unwrap();
        });
        ready_rx.recv().unwrap();
        storage
            .writer
            .execute_batch(
                "CREATE TABLE junk (b BLOB);
                 BEGIN;
                 WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 160)
                   INSERT INTO junk SELECT randomblob(65536) FROM n;
                 COMMIT;",
            )
            .unwrap();
        let (tx, rx) = mpsc::channel();
        let handle = spawn(storage, fake(&fx), Default::default(), move |_, _| {
            tx.send(()).unwrap();
        });
        handle.request(Origen::EscaneoInicial);
        rx.recv_timeout(WAIT).unwrap();
        assert!(wal() > LIMIT, "con el lector abierto no se puede");
        hold_tx.send(()).unwrap();
        holder.join().unwrap();
        // El siguiente trabajo no escribe nada (archivo sin cambios), pero deja al escritor libre.
        handle.rescan([fx.root.join("a.log")].into());
        rx.recv_timeout(WAIT).unwrap();
        let deadline = std::time::Instant::now() + WAIT;
        while wal() > LIMIT {
            assert!(
                std::time::Instant::now() < deadline,
                "el WAL sigue en {}",
                wal()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
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
        let handle = spawn(writer, providers, generation.clone(), move |_, s| {
            done_tx.send(s.clone()).unwrap();
        });
        handle.request(Origen::EscaneoInicial);
        assert_eq!(started_rx.recv_timeout(WAIT).unwrap(), "espera");
        // Espera a que a.log esté confirmado: el trabajo llega con el escritor ya esperando a b.log.
        let deadline = std::time::Instant::now() + WAIT;
        while calls(&fx) < 1 {
            assert!(std::time::Instant::now() < deadline, "a.log no se confirmó");
            std::thread::sleep(Duration::from_millis(10));
        }
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
            fn committed(&mut self, conn: &mut Connection, touched: &Touched) -> Result<()> {
                let before = self.inner.generation.get();
                let visible = calls(self.fx);
                self.inner.committed(conn, touched)?;
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
                on_change: &mut |_, _| {},
            },
            fx: &fx,
            seen: Vec::new(),
        };
        crate::ingest::scan_with_hooks(&mut writer.writer, &providers, &mut check).unwrap();
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

    fn corrupt() -> anyhow::Error {
        anyhow::Error::new(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
            Some("database disk image is malformed".into()),
        ))
        .context("falló la consulta")
    }

    #[test]
    fn una_base_danada_se_reconstruye_y_la_app_sigue() {
        assert!(db::is_corruption(&corrupt()));
        assert!(!db::is_corruption(&anyhow::anyhow!("otro error")));
        let (fx, storage) = fixture(&[("a.log", "m1\nm2\n")]);
        let readers = readers_of(&storage);
        let (tx, rx) = mpsc::channel();
        let handle = IngestThread::spawn(
            storage,
            readers.clone(),
            Vec::new,
            fake(&fx),
            Default::default(),
            move |origen, stats| tx.send((origen, stats.records)).unwrap(),
        );
        handle.request(Origen::EscaneoInicial);
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), (Origen::EscaneoInicial, 4));
        let before = crate::commands::current(&readers).unwrap();
        // Un lector se encuentra la base dañada: se pide reconstruir y se relee todo.
        handle.report(&corrupt());
        handle.report(&anyhow::anyhow!("un error cualquiera no reconstruye"));
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), (Origen::EscaneoInicial, 4));
        let after = crate::commands::current(&readers).unwrap();
        assert!(!Arc::ptr_eq(&before, &after), "lectores nuevos");
        assert_eq!(calls(&fx), 2);
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "una sola reconstrucción"
        );
    }

    /// Volcado de los datos importados por un lector del conjunto vigente.
    fn snapshot(readers: &SharedReaders) -> String {
        crate::commands::current(readers)
            .unwrap()
            .with(|c| {
                let mut stmt = c.prepare(
                    "SELECT message_id, session_id, ts, cost_usd, day FROM calls ORDER BY 1",
                )?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok(format!(
                            "{}|{}|{}|{}|{}",
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, f64>(3)?,
                            r.get::<_, i64>(4)?
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows.join("\n"))
            })
            .unwrap()
    }

    #[test]
    fn reconstruir_en_memoria_o_en_disco_deja_los_mismos_datos() {
        let (fx, storage) = fixture(&[("a.log", "m1\nm2\n"), ("b.log", "m3\n")]);
        let path = match &storage.mode {
            CacheMode::Disco(p) => p.clone(),
            CacheMode::Memoria => unreachable!(),
        };
        let readers = readers_of(&storage);
        let (tx, rx) = mpsc::channel();
        let handle = IngestThread::spawn(
            storage,
            readers.clone(),
            Vec::new,
            fake(&fx),
            Default::default(),
            move |origen, stats| tx.send((origen, stats.records)).unwrap(),
        );
        handle.request(Origen::EscaneoInicial);
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), (Origen::EscaneoInicial, 6));
        let reference = snapshot(&readers);

        // A memoria: la caché en disco desaparece y los datos se vuelven a leer.
        handle.rebuild(Some(CacheMode::Memoria)).unwrap();
        assert!(!path.exists(), "caché borrada");
        for suffix in ["-wal", "-shm"] {
            assert!(!PathBuf::from(format!("{}{suffix}", path.display())).exists());
        }
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), (Origen::EscaneoInicial, 6));
        assert_eq!(snapshot(&readers), reference);

        // De vuelta a disco: se crea y se rellena.
        handle
            .rebuild(Some(CacheMode::Disco(path.clone())))
            .unwrap();
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), (Origen::EscaneoInicial, 6));
        assert!(path.exists());
        assert_eq!(snapshot(&readers), reference);
        assert_eq!(calls(&fx), 3, "en el archivo nuevo");
    }

    #[test]
    fn las_consultas_responden_durante_la_reconstruccion() {
        let files: Vec<(String, String)> = (0..20)
            .map(|i| (format!("{i}.log"), format!("m{i}\n")))
            .collect();
        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let (fx, storage) = fixture(&refs);
        let readers = readers_of(&storage);
        let providers: Vec<Box<dyn Provider>> = vec![Box::new(Fake {
            root: fx.root.clone(),
            gate: None,
            delay: Duration::from_millis(20),
        })];
        let (tx, rx) = mpsc::channel();
        let handle = IngestThread::spawn(
            storage,
            readers.clone(),
            Vec::new,
            providers,
            Default::default(),
            move |origen, _| tx.send(origen).unwrap(),
        );
        handle.request(Origen::EscaneoInicial);
        rx.recv_timeout(WAIT).unwrap();
        handle.rebuild(None).unwrap();
        // Mientras relee los 20 archivos, las consultas responden (con lo releído hasta ahora).
        let mut answered = 0;
        while rx.try_recv().is_err() {
            let n = crate::commands::current(&readers)
                .unwrap()
                .with(
                    |c| Ok(c.query_row("SELECT COUNT(*) FROM calls", [], |r| r.get::<_, i64>(0))?),
                )
                .expect("la consulta responde sin error");
            assert!((0..=20).contains(&n));
            answered += 1;
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(answered > 0);
    }

    #[test]
    fn un_trabajo_fallido_no_detiene_el_hilo() {
        let (fx, writer) = fixture(&[("a.log", "m1\n")]);
        writer
            .writer
            .execute_batch(
                "CREATE TRIGGER sin_precios BEFORE INSERT ON prices BEGIN SELECT RAISE(ABORT, 'no'); END;",
            )
            .unwrap();
        let (tx, rx) = mpsc::channel();
        let handle = spawn(writer, fake(&fx), Default::default(), move |origen, _| {
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
