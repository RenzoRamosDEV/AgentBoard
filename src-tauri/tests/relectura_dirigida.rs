//! Relectura dirigida (`ingest::scan_paths`): tras cualquier secuencia de cambios en los logs,
//! releer solo las rutas tocadas deja la base igual que leerlo todo desde cero.

#[path = "../benches/common/mod.rs"]
mod common;

use agentboard_lib::{db, ingest};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Generador congruencial con semilla fija: la secuencia es siempre la misma.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % n
    }
}

fn logs(root: &Path) -> Vec<PathBuf> {
    ingest::walk(&root.join("claude_code"))
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect()
}

/// Una sesión con el `cwd`, la rama y el modelo que tiene guardados. Las líneas nuevas de una
/// sesión existente los repiten: con valores distintos repartidos entre archivos, el proyecto, la
/// rama y el modelo de la sesión dependen del orden de lectura también en el escaneo completo
/// incremental (el primero que fija el proyecto, el último que escribe la rama o el modelo).
struct SessionInfo {
    id: String,
    cwd: String,
    branch: Option<String>,
    model: String,
}

impl SessionInfo {
    fn fresh(id: String) -> Self {
        SessionInfo {
            id,
            cwd: "/home/dev/Proyectos/proyecto-00".into(),
            branch: Some("main".into()),
            model: "claude-sonnet-4-5".into(),
        }
    }
}

/// La sesión de la primera línea del log que la tenga, con sus valores guardados en `conn`.
fn session_of(conn: &rusqlite::Connection, path: &Path) -> SessionInfo {
    let id = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find_map(|v| v["sessionId"].as_str().map(str::to_string))
        .unwrap_or_else(|| path.file_stem().unwrap().to_string_lossy().to_string());
    let stored = conn
        .query_row(
            "SELECT p.cwd, s.git_branch, s.model FROM sessions s
             LEFT JOIN projects p ON p.id = s.project_id WHERE s.id = ?1",
            [&id],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .ok();
    let mut info = SessionInfo::fresh(id);
    if let Some((cwd, branch, model)) = stored {
        info.cwd = cwd.unwrap_or(info.cwd);
        info.branch = branch;
        info.model = model.unwrap_or(info.model);
    }
    info
}

/// `n` líneas nuevas de la sesión `s`, con su `cwd`, su rama y su modelo.
fn lines_of(s: &SessionInfo, first: usize, n: usize) -> String {
    let branch = match &s.branch {
        Some(b) => format!(r#""gitBranch":{},"#, serde_json::json!(b)),
        None => String::new(),
    };
    common::live_lines(&s.id, first, n)
        .replace(
            r#""cwd":"/home/dev/Proyectos/proyecto-00""#,
            &format!(r#""cwd":{}"#, serde_json::json!(s.cwd)),
        )
        .replace(r#""gitBranch":"main","#, &branch)
        .replace(
            r#""model":"claude-sonnet-4-5""#,
            &format!(r#""model":{}"#, serde_json::json!(s.model)),
        )
}

fn append(path: &Path, text: &str) {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}

fn from_scratch(root: &Path) -> String {
    let mut conn = db::open_in_memory().unwrap();
    ingest::scan_all(&mut conn, &common::providers(root)).unwrap();
    common::fingerprint(&conn).unwrap()
}

#[test]
fn releer_solo_lo_tocado_deja_la_misma_huella_que_leer_todo() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("2m");
    if !common::generate_fixture("2m", 11, &root) {
        eprintln!("python3 no disponible: se salta la relectura dirigida sobre el fixture 2m");
        return;
    }
    let providers = common::providers(&root);
    let mut conn = db::open_in_memory().unwrap();
    ingest::scan_all(&mut conn, &providers).unwrap();
    // Referencia intermedia: el escaneo completo incremental de siempre, con sus propios proveedores.
    let full_providers = common::providers(&root);
    let mut full = db::open_in_memory().unwrap();
    ingest::scan_all(&mut full, &full_providers).unwrap();
    let mut rng = Lcg(0x5eed_f00d);
    let mut next_id = 0;
    let mut lines = |session: &SessionInfo, n: usize| {
        next_id += n;
        lines_of(session, next_id - n, n)
    };
    let fresh = SessionInfo::fresh;
    // Componente a componente, como las rutas de `logs`: en Windows un "/" dentro de un `join`
    // daría otra clave para el mismo archivo y la relectura dirigida no lo reconocería.
    let new_dir = root
        .join("claude_code")
        .join("-home-dev-Proyectos-proyecto-00");
    std::fs::create_dir_all(&new_dir).unwrap();
    // Pasos en los que el escaneo completo incremental (y por tanto la relectura dirigida) deja lo
    // mismo que leer desde cero. Hay secuencias forzadas (sesiones repartidas entre archivos que se
    // truncan o rotan) en las que el propio escaneo incremental ya difiere: se comprueba la
    // relectura dirigida contra él y se exige que sean excepción.
    let mut same_as_scratch = 0;
    const STEPS: usize = 24;
    for step in 0..STEPS {
        let files = logs(&root);
        let target = files[rng.next(files.len())].clone();
        let (op, touched): (&str, Vec<PathBuf>) = match step % 6 {
            0 => {
                append(
                    &target,
                    &lines(&session_of(&full, &target), 1 + rng.next(3)),
                );
                ("añadir", vec![target])
            }
            1 => {
                let path = new_dir.join(format!("nueva-{step}.jsonl"));
                append(&path, &lines(&fresh(format!("nueva-{step}")), 2));
                ("crear", vec![path])
            }
            2 => {
                // Una sesión repartida entre dos archivos: el nuevo continúa la de `target`.
                let path = new_dir.join(format!("continua-{step}.jsonl"));
                append(&path, &lines(&session_of(&full, &target), 2));
                ("sesión en dos archivos", vec![path])
            }
            3 => {
                std::fs::remove_file(&target).unwrap();
                ("borrar", vec![target])
            }
            4 => {
                let tmp = target.with_extension("tmp");
                std::fs::write(&tmp, lines(&fresh(format!("rotada-{step}")), 1)).unwrap();
                std::fs::rename(&tmp, &target).unwrap();
                ("rotar", vec![target])
            }
            _ => {
                let text = std::fs::read_to_string(&target).unwrap();
                let keep: String = text
                    .lines()
                    .take(text.lines().count() / 2)
                    .map(|l| format!("{l}\n"))
                    .collect();
                std::fs::write(&target, keep).unwrap();
                ("truncar", vec![target])
            }
        };
        let paths: BTreeSet<PathBuf> = touched.into_iter().collect();
        let stats = ingest::scan_paths(&mut conn, &providers, &paths).unwrap();
        assert_eq!(stats.errors, 0, "paso {step} ({op})");
        assert!(!stats.full);
        ingest::scan_all(&mut full, &full_providers).unwrap();
        let incremental = common::fingerprint(&full).unwrap();
        let scratch = from_scratch(&root);
        assert_eq!(
            common::fingerprint(&conn).unwrap(),
            incremental,
            "relectura dirigida frente al escaneo completo incremental, paso {step} ({op}): {paths:?}"
        );
        if incremental == scratch {
            same_as_scratch += 1;
        } else {
            eprintln!(
                "paso {step} ({op}): el escaneo completo incremental ya difiere de leer desde cero"
            );
        }
    }
    assert!(
        same_as_scratch >= STEPS - 4,
        "solo {same_as_scratch} de {STEPS} pasos iguales a leer desde cero"
    );
}
