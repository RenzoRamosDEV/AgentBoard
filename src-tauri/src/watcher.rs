//! Vigilante de archivos: observa las carpetas de logs de los agentes, agrupa por archivo los
//! eventos de cada ráfaga y pide al hilo de ingesta la relectura dirigida de los archivos tocados
//! (los offsets de `file_state` evitan duplicar). Sin eventos no se despierta: las carpetas que aún
//! no existen se esperan vigilando su antepasado existente más profundo.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use crate::ingest_thread::{IngestHandle, Origen};
use crate::providers::Provider;
use anyhow::Result;
use notify::event::ModifyKind;
use notify::{EventKind, RecursiveMode, Watcher};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Calma tras el último evento de un archivo antes de releerlo.
const QUIET: Duration = Duration::from_millis(200);
/// Espera máxima desde el primer evento pendiente de un archivo que no para de cambiar.
const MAX_WAIT: Duration = Duration::from_secs(1);
/// Rechequeo de una raíz que existe pero no se pudo vigilar (p. ej. `max_user_watches`): sin él
/// esa raíz no se actualizaría nunca. Es el único temporizador del vigilante.
const RETRY: Duration = Duration::from_secs(30);

/// Fábrica de providers; se inyecta para poder probar el vigilante con carpetas de test.
pub type Providers = Arc<dyn Fn() -> Vec<Box<dyn Provider>> + Send + Sync>;

/// Lanza el vigilante en su propio hilo.
pub fn spawn(providers: Providers, ingest: IngestHandle) {
    std::thread::spawn(move || {
        if let Err(e) = run(providers, ingest) {
            tracing::error!("agentboard: el vigilante se detuvo: {e:#}");
        }
    });
}

/// La base SQLite de una ruta auxiliar (`-wal`, `-shm`, `-journal`): sus cambios llegan por ahí.
fn normalize(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    for suffix in ["-wal", "-shm", "-journal"] {
        if let Some(base) = name.strip_suffix(suffix).filter(|b| !b.is_empty()) {
            return path.with_file_name(base);
        }
    }
    path.to_path_buf()
}

fn under_root(path: &Path, providers: &[Box<dyn Provider>]) -> bool {
    providers
        .iter()
        .any(|p| p.log_roots().iter().any(|r| path.starts_with(r)))
}

fn recognized(path: &Path, providers: &[Box<dyn Provider>]) -> bool {
    providers
        .iter()
        .any(|p| p.log_roots().iter().any(|r| path.starts_with(r)) && p.matches(path))
}

/// Archivos de un evento que hay que releer, ordenados: sin los de solo acceso ni los que ningún
/// proveedor reconoce bajo sus raíces; una carpeta creada o movida dentro de una raíz aporta los
/// archivos reconocidos que ya contenga (pudieron escribirse antes de vigilarla).
pub fn relevant(event: &notify::Event, providers: &[Box<dyn Provider>]) -> Vec<PathBuf> {
    if matches!(event.kind, EventKind::Access(_)) {
        return Vec::new();
    }
    let new_entry = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(_))
    );
    let mut out = BTreeSet::new();
    for path in event.paths.iter().map(|p| normalize(p)) {
        if recognized(&path, providers) {
            out.insert(path);
        } else if new_entry && path.is_dir() && under_root(&path, providers) {
            out.extend(
                crate::ingest::walk(&path)
                    .into_iter()
                    .filter(|f| recognized(f, providers)),
            );
        }
    }
    out.into_iter().collect()
}

/// Agrupa los eventos por archivo: uno está listo tras `QUIET` sin eventos o `MAX_WAIT` desde su
/// primer evento pendiente. El reloj es un parámetro.
#[derive(Debug, Default)]
pub struct Debouncer {
    pending: HashMap<PathBuf, (Instant, Instant)>,
}

impl Debouncer {
    pub fn add(&mut self, path: PathBuf, now: Instant) {
        self.pending
            .entry(path)
            .and_modify(|(_, last)| *last = now)
            .or_insert((now, now));
    }

    fn deadline((first, last): &(Instant, Instant)) -> Instant {
        (*last + QUIET).min(*first + MAX_WAIT)
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.pending.values().map(Self::deadline).min()
    }

    pub fn take_ready(&mut self, now: Instant) -> Vec<PathBuf> {
        let mut ready: Vec<PathBuf> = self
            .pending
            .iter()
            .filter(|(_, times)| Self::deadline(times) <= now)
            .map(|(path, _)| path.clone())
            .collect();
        for path in &ready {
            self.pending.remove(path);
        }
        ready.sort();
        ready
    }
}

/// Qué vigilar para cada raíz: la raíz entera si existe; si no, sin recursión, su antepasado
/// existente más profundo (lo que cuelgue de una raíz vigilada entera ya está cubierto).
pub fn watch_plan(roots: &[PathBuf]) -> BTreeMap<PathBuf, RecursiveMode> {
    let mut plan = BTreeMap::new();
    for root in roots {
        if root.is_dir() {
            plan.insert(root.clone(), RecursiveMode::Recursive);
        } else if let Some(ancestor) = root.ancestors().skip(1).find(|a| a.is_dir()) {
            plan.entry(ancestor.to_path_buf())
                .or_insert(RecursiveMode::NonRecursive);
        }
    }
    let whole: Vec<PathBuf> = plan
        .iter()
        .filter(|(_, mode)| **mode == RecursiveMode::Recursive)
        .map(|(path, _)| path.clone())
        .collect();
    plan.retain(|path, mode| {
        *mode == RecursiveMode::Recursive || !whole.iter().any(|w| path.starts_with(w))
    });
    plan
}

/// Lo que el vigilante tiene registrado en `notify` y las raíces que no se pudieron vigilar.
struct Watches {
    roots: Vec<PathBuf>,
    active: BTreeMap<PathBuf, RecursiveMode>,
    failed: BTreeSet<PathBuf>,
}

impl Watches {
    /// Ajusta las vigilancias al plan actual. Devuelve las raíces que pasan a vigilarse enteras.
    fn update(&mut self, watcher: &mut impl Watcher) -> Vec<PathBuf> {
        let plan = watch_plan(&self.roots);
        for (path, mode) in &self.active {
            if plan.get(path) != Some(mode) {
                let _ = watcher.unwatch(path);
            }
        }
        self.active
            .retain(|path, mode| plan.get(path) == Some(mode));
        self.failed.retain(|root| plan.contains_key(root));
        let mut promoted = Vec::new();
        for (path, mode) in plan {
            if self.active.contains_key(&path) || self.failed.contains(&path) {
                continue;
            }
            match watcher.watch(&path, mode) {
                Ok(()) => {
                    if mode == RecursiveMode::Recursive {
                        promoted.push(path.clone());
                    }
                    self.active.insert(path, mode);
                }
                Err(e) => {
                    tracing::warn!(ruta = %path.display(), "agentboard: no se pudo vigilar {}: {e}", path.display());
                    if mode == RecursiveMode::Recursive {
                        self.failed.insert(path);
                    }
                }
            }
        }
        promoted
    }

    /// ¿Crea o quita el evento una carpeta del camino de alguna raíz (o la raíz misma)?
    fn affects_plan(&self, event: &notify::Event) -> bool {
        matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
        ) && event
            .paths
            .iter()
            .any(|p| self.roots.iter().any(|r| r.starts_with(p)))
    }

    /// Reintenta vigilar las raíces que fallaron; devuelve todas las reintentadas, que se releen
    /// enteras (sin vigilancia no llegan sus eventos).
    fn retry(&mut self, watcher: &mut impl Watcher) -> Vec<PathBuf> {
        let failed = std::mem::take(&mut self.failed);
        let mut retried = Vec::new();
        for root in failed {
            match watcher.watch(&root, RecursiveMode::Recursive) {
                Ok(()) => {
                    self.active.insert(root.clone(), RecursiveMode::Recursive);
                }
                Err(_) => {
                    self.failed.insert(root.clone());
                }
            }
            retried.push(root);
        }
        retried
    }
}

/// Archivos reconocidos bajo `root`.
fn files_under(root: &Path, providers: &[Box<dyn Provider>]) -> BTreeSet<PathBuf> {
    crate::ingest::walk(root)
        .into_iter()
        .filter(|f| recognized(f, providers))
        .collect()
}

fn run(providers: Providers, ingest: IngestHandle) -> Result<()> {
    let providers = providers();
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let _ = tx.send(res);
    })?;
    let mut watches = Watches {
        roots: providers.iter().flat_map(|p| p.log_roots()).collect(),
        active: BTreeMap::new(),
        failed: BTreeSet::new(),
    };
    // Las raíces que ya existen las lee el escaneo inicial.
    watches.update(&mut watcher);
    let mut debouncer = Debouncer::default();
    let mut retry_at = (!watches.failed.is_empty()).then(|| Instant::now() + RETRY);
    loop {
        let deadline = match (debouncer.next_deadline(), retry_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let received = match deadline {
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(at) => rx.recv_timeout(at.saturating_duration_since(Instant::now())),
        };
        match received {
            Ok(Ok(event)) if event.need_rescan() => {
                tracing::warn!("agentboard: se perdieron eventos del sistema; escaneo completo");
                ingest.request(Origen::Vigilante);
            }
            Ok(Ok(event)) => {
                let now = Instant::now();
                for path in relevant(&event, &providers) {
                    debouncer.add(path, now);
                }
                if watches.affects_plan(&event) {
                    for root in watches.update(&mut watcher) {
                        // Lo escrito antes de vigilarla no llegó como evento.
                        let files = files_under(&root, &providers);
                        if !files.is_empty() {
                            ingest.rescan(files);
                        }
                    }
                    if !watches.failed.is_empty() && retry_at.is_none() {
                        retry_at = Some(now + RETRY);
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("agentboard: error del vigilante; escaneo completo: {e}");
                ingest.request(Origen::Vigilante);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        let ready = debouncer.take_ready(Instant::now());
        if !ready.is_empty() {
            ingest.rescan(ready.into_iter().collect());
        }
        if retry_at.is_some_and(|at| at <= Instant::now()) {
            let mut files = BTreeSet::new();
            for root in watches.retry(&mut watcher) {
                files.extend(files_under(&root, &providers));
            }
            if !files.is_empty() {
                ingest.rescan(files);
            }
            retry_at = (!watches.failed.is_empty()).then(|| Instant::now() + RETRY);
        }
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
        let storage =
            db::Storage::open(db::CacheMode::Disco(dir.path().join("cache.sqlite"))).unwrap();
        let readers = storage.readers(1).unwrap();
        let shared = Arc::new(std::sync::RwLock::new(Arc::new(
            storage.readers(1).unwrap(),
        )));
        let root2 = root.clone();
        let providers: Providers = Arc::new(move || {
            vec![Box::new(ClaudeCode::with_roots(vec![root2.clone()])) as Box<dyn Provider>]
        });
        let ingest = IngestThread::spawn(
            storage,
            shared,
            Vec::new,
            providers(),
            Default::default(),
            |_, _| {},
        );
        let providers2 = providers.clone();
        std::thread::spawn(move || {
            let _ = run(providers2, ingest);
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
        assert!(ok, "no leyó la carpeta aparecida después (sin rechequeo)");
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

        let storage =
            db::Storage::open(db::CacheMode::Disco(dir.path().join("cache.sqlite"))).unwrap();
        let readers = storage.readers(1).unwrap();
        let shared = Arc::new(std::sync::RwLock::new(Arc::new(
            storage.readers(1).unwrap(),
        )));
        let root2 = root.clone();
        let providers: Providers = Arc::new(move || {
            vec![Box::new(ClaudeCode::with_roots(vec![root2.clone()])) as Box<dyn Provider>]
        });
        let calls = Arc::new(AtomicUsize::new(0));

        let calls2 = calls.clone();
        let ingest = IngestThread::spawn(
            storage,
            shared,
            Vec::new,
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
            let _ = run(providers2, ingest);
        });

        let count = |sql: &str| {
            readers
                .with(|c| Ok(c.query_row(sql, [], |r| r.get::<_, i64>(0))?))
                .unwrap()
        };
        // Deja que el vigilante registre su carpeta.
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

    #[test]
    fn un_archivo_no_reconocido_no_provoca_relecturas() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("projects");
        std::fs::create_dir_all(root.join("p")).unwrap();
        let storage =
            db::Storage::open(db::CacheMode::Disco(dir.path().join("cache.sqlite"))).unwrap();
        let shared = Arc::new(std::sync::RwLock::new(Arc::new(
            storage.readers(1).unwrap(),
        )));
        let root2 = root.clone();
        let providers: Providers = Arc::new(move || {
            vec![Box::new(ClaudeCode::with_roots(vec![root2.clone()])) as Box<dyn Provider>]
        });
        let scans = Arc::new(AtomicUsize::new(0));
        let scans2 = scans.clone();
        let ingest = IngestThread::spawn(
            storage,
            shared,
            Vec::new,
            providers(),
            Default::default(),
            move |_, _| {
                scans2.fetch_add(1, Ordering::SeqCst);
            },
        );
        let providers2 = providers.clone();
        std::thread::spawn(move || {
            let _ = run(providers2, ingest);
        });
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(root.join("p").join("notas.txt"), "hola\n").unwrap();
        let _ = std::fs::read_to_string(root.join("p").join("notas.txt"));
        std::thread::sleep(Duration::from_millis(800));
        assert_eq!(scans.load(Ordering::SeqCst), 0, "no debe releer nada");
    }
}
#[cfg(test)]
mod pure_tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, EventKind, ModifyKind, RemoveKind, RenameMode};
    use std::path::Path;

    /// Proveedor de prueba: reconoce `*.jsonl` y `opencode.db` bajo su raíz.
    struct Fake(PathBuf);

    impl Provider for Fake {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn name(&self) -> &'static str {
            "Fake"
        }
        fn log_roots(&self) -> Vec<PathBuf> {
            vec![self.0.clone()]
        }
        fn matches(&self, path: &Path) -> bool {
            path.extension().is_some_and(|e| e == "jsonl")
                || path.file_name().is_some_and(|n| n == "opencode.db")
        }
        fn parse_line(&self, _: &Path, _: &str) -> anyhow::Result<Vec<crate::providers::Record>> {
            Ok(Vec::new())
        }
    }

    fn event(kind: EventKind, paths: &[&Path]) -> notify::Event {
        let mut e = notify::Event::new(kind);
        for p in paths {
            e = e.add_path(p.to_path_buf());
        }
        e
    }

    fn providers(root: &Path) -> Vec<Box<dyn Provider>> {
        vec![Box::new(Fake(root.to_path_buf()))]
    }

    #[test]
    fn relevant_filtra_y_normaliza() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("logs");
        std::fs::create_dir_all(&root).unwrap();
        let p = providers(&root);
        let log = root.join("s.jsonl");
        let modify = EventKind::Modify(ModifyKind::Any);
        assert_eq!(relevant(&event(modify, &[&log]), &p), vec![log.clone()]);
        assert!(
            relevant(&event(EventKind::Access(AccessKind::Any), &[&log]), &p).is_empty(),
            "Access se descarta"
        );
        assert!(relevant(&event(modify, &[&root.join("notas.txt")]), &p).is_empty());
        assert!(
            relevant(&event(modify, &[&dir.path().join("fuera.jsonl")]), &p).is_empty(),
            "fuera de las raíces"
        );
        let db = root.join("opencode.db");
        for suffix in ["-wal", "-shm", "-journal"] {
            let side = root.join(format!("opencode.db{suffix}"));
            assert_eq!(relevant(&event(modify, &[&side]), &p), vec![db.clone()]);
        }
        let gone = root.join("borrado.jsonl");
        assert_eq!(
            relevant(&event(EventKind::Remove(RemoveKind::File), &[&gone]), &p),
            vec![gone.clone()]
        );
        let moved = root.join("movido.jsonl");
        assert_eq!(
            relevant(
                &event(
                    EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                    &[&gone, &moved]
                ),
                &p
            ),
            vec![gone, moved]
        );
    }

    #[test]
    fn una_carpeta_creada_aporta_los_archivos_que_ya_tiene() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("logs");
        let sub = root.join("p/q");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("a.jsonl"), "x\n").unwrap();
        std::fs::write(sub.join("b.txt"), "x\n").unwrap();
        std::fs::write(root.join("p/c.jsonl"), "x\n").unwrap();
        let got = relevant(
            &event(EventKind::Create(CreateKind::Folder), &[&root.join("p")]),
            &providers(&root),
        );
        assert_eq!(got, vec![root.join("p/c.jsonl"), sub.join("a.jsonl")]);
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn una_rafaga_de_un_archivo_sale_una_vez_a_los_200_ms_del_ultimo_evento() {
        let t0 = Instant::now();
        let a = PathBuf::from("/l/a.jsonl");
        let mut d = Debouncer::default();
        assert_eq!(d.next_deadline(), None);
        d.add(a.clone(), t0);
        d.add(a.clone(), t0 + ms(50));
        assert_eq!(d.next_deadline(), Some(t0 + ms(250)));
        assert!(d.take_ready(t0 + ms(249)).is_empty());
        assert_eq!(d.take_ready(t0 + ms(250)), vec![a.clone()]);
        assert!(d.take_ready(t0 + ms(400)).is_empty(), "una sola vez");
        assert_eq!(d.next_deadline(), None);

        let mut d = Debouncer::default();
        d.add(a.clone(), t0);
        assert!(d.take_ready(t0 + ms(199)).is_empty());
        assert_eq!(d.take_ready(t0 + ms(200)), vec![a.clone()]);
        let mut d = Debouncer::default();
        d.add(a.clone(), t0);
        assert_eq!(d.take_ready(t0 + ms(201)), vec![a]);
    }

    #[test]
    fn un_archivo_que_no_para_de_crecer_sale_al_segundo_de_su_primer_evento() {
        let t0 = Instant::now();
        let a = PathBuf::from("/l/a.jsonl");
        let mut d = Debouncer::default();
        for k in 0..10 {
            d.add(a.clone(), t0 + ms(100 * k));
        }
        assert_eq!(d.next_deadline(), Some(t0 + ms(1000)));
        assert!(d.take_ready(t0 + ms(999)).is_empty());
        assert_eq!(d.take_ready(t0 + ms(1000)), vec![a.clone()]);
        let mut d = Debouncer::default();
        for k in 0..10 {
            d.add(a.clone(), t0 + ms(100 * k));
        }
        assert_eq!(d.take_ready(t0 + ms(1001)), vec![a.clone()]);
        // Tras salir, el siguiente evento empieza otra espera.
        d.add(a.clone(), t0 + ms(1050));
        assert_eq!(d.next_deadline(), Some(t0 + ms(1250)));
    }

    #[test]
    fn dos_archivos_salen_por_separado_y_ordenados() {
        let t0 = Instant::now();
        let (a, b, c) = (
            PathBuf::from("/l/a.jsonl"),
            PathBuf::from("/l/b.jsonl"),
            PathBuf::from("/l/c.jsonl"),
        );
        let mut d = Debouncer::default();
        d.add(b.clone(), t0);
        d.add(a.clone(), t0 + ms(10));
        d.add(c.clone(), t0 + ms(150));
        assert_eq!(d.next_deadline(), Some(t0 + ms(200)));
        assert_eq!(d.take_ready(t0 + ms(200)), vec![b.clone()]);
        assert_eq!(d.next_deadline(), Some(t0 + ms(210)));
        assert_eq!(d.take_ready(t0 + ms(360)), vec![a, c]);
        assert_eq!(d.next_deadline(), None);
    }

    #[test]
    fn el_plan_vigila_el_antepasado_existente_y_promueve_cada_nivel() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_path_buf();
        let root = base.join("a/b/c");
        let plan = |roots: &[PathBuf]| watch_plan(roots);
        assert_eq!(
            plan(std::slice::from_ref(&root)),
            BTreeMap::from([(base.clone(), RecursiveMode::NonRecursive)])
        );
        std::fs::create_dir(base.join("a")).unwrap();
        assert_eq!(
            plan(std::slice::from_ref(&root)),
            BTreeMap::from([(base.join("a"), RecursiveMode::NonRecursive)])
        );
        std::fs::create_dir(base.join("a/b")).unwrap();
        assert_eq!(
            plan(std::slice::from_ref(&root)),
            BTreeMap::from([(base.join("a/b"), RecursiveMode::NonRecursive)])
        );
        std::fs::create_dir(&root).unwrap();
        let other = base.join("otra");
        std::fs::create_dir(&other).unwrap();
        assert_eq!(
            plan(&[root.clone(), other.clone()]),
            BTreeMap::from([
                (root, RecursiveMode::Recursive),
                (other, RecursiveMode::Recursive)
            ])
        );
    }

    #[test]
    fn una_raiz_vigilada_entera_no_se_vigila_ademas_como_antepasado() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_path_buf();
        let parent = base.join("p");
        std::fs::create_dir(&parent).unwrap();
        let pending = parent.join("hija");
        assert_eq!(
            watch_plan(&[parent.clone(), pending]),
            BTreeMap::from([(parent, RecursiveMode::Recursive)])
        );
    }
}
