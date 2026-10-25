//! Medición de rendimiento sobre una carpeta de fixtures sintéticos (nunca los logs reales):
//! escaneo frío, refresco en vivo, consultas del Resumen y memoria pico.
//!
//! Uso (desde `src-tauri/`):
//!   cargo run --release --example perf_scan -- --fixtures target/perf-fixtures/50m
//!   cargo run --release --example perf_scan -- --fixtures <dir> --phase live --repeat 200
//!   cargo build --profile bench-fast --example perf_scan   (pasos intermedios, sin LTO)
//!   ./target/bench-fast/examples/perf_scan --fixtures <dir> --phase watch [--events 500]
//!   ./target/bench-fast/examples/perf_scan --fixtures <dir> --phase idle --idle-secs 60

#[path = "../benches/common/mod.rs"]
mod common;

use agentboard_lib::ingest_thread::{IngestHandle, IngestThread, Origen};
use agentboard_lib::{db, ingest, queries, sessions, telemetry, watcher};
use anyhow::{bail, Context, Result};
use common::{median, LiveFile, Phase, Reader};
use rusqlite::Connection;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const WARM_RUNS: usize = 5;

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

fn row(phase: &str, metric: &str, value: impl std::fmt::Display) {
    println!("{phase:<10} | {metric:<44} | {value}");
}

fn cold(
    conn: &mut Connection,
    providers: &[Box<dyn agentboard_lib::providers::Provider>],
    print: bool,
) -> Result<()> {
    let t = Instant::now();
    let stats = ingest::scan_all(conn, providers)?;
    let took = t.elapsed();
    if stats.errors > 0 {
        bail!("el escaneo frío tuvo {} errores", stats.errors);
    }
    let phase = if print { "cold" } else { "setup" };
    row(phase, "escaneo frío", ms(took));
    row(
        phase,
        "archivos / líneas / registros",
        format!("{} / {} / {}", stats.files, stats.lines, stats.records),
    );
    Ok(())
}

/// `repeat` refrescos de `live_lines` líneas nuevas en un archivo de actividad simulada; `round`
/// separa los ids de varias tandas sobre el mismo archivo.
fn refreshes(
    conn: &mut Connection,
    providers: &[Box<dyn agentboard_lib::providers::Provider>],
    args: &common::Args,
    (guard, session): &(LiveFile, String),
    round: usize,
    after: &dyn Fn(),
    checkpoint: Option<&db::Target>,
) -> Result<(Vec<Duration>, usize)> {
    let mut times = Vec::with_capacity(args.repeat);
    let mut records = 0;
    for r in 0..args.repeat {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&guard.path)
            .with_context(|| format!("no se pudo escribir {}", guard.path.display()))?;
        let first = (round * args.repeat + r) * args.live_lines;
        f.write_all(common::live_lines(session, first, args.live_lines).as_bytes())?;
        drop(f);
        let t = Instant::now();
        let stats = ingest::scan_all(conn, providers)?;
        times.push(t.elapsed());
        records += stats.records;
        // Como el hilo de ingesta: con el escritor libre, el WAL grande se trunca.
        if let Some(target) = checkpoint {
            let _ = match db::checkpoint_if_large(conn, target)? {
                db::Checkpoint::Done => CHECKPOINTS_DONE.fetch_add(1, Ordering::Relaxed),
                db::Checkpoint::Busy => CHECKPOINTS_BUSY.fetch_add(1, Ordering::Relaxed),
                db::Checkpoint::NotNeeded => 0,
            };
        }
        after();
    }
    Ok((times, records))
}

fn live_file(args: &common::Args) -> Result<(LiveFile, String)> {
    let dir = args
        .fixtures
        .join("claude_code/-home-dev-Proyectos-proyecto-00");
    std::fs::create_dir_all(&dir)?;
    let session = format!("perf-live-{}", std::process::id());
    Ok((LiveFile::new(dir.join(format!("{session}.jsonl"))), session))
}

fn live(
    conn: &mut Connection,
    providers: &[Box<dyn agentboard_lib::providers::Provider>],
    args: &common::Args,
) -> Result<()> {
    let live = live_file(args)?;
    let (times, records) = refreshes(conn, providers, args, &live, 0, &|| {}, None)?;
    let total: Duration = times.iter().sum();
    row(
        "live",
        "refresco (mediana)",
        format!(
            "{} ({} × {} líneas)",
            ms(median(&times)),
            args.repeat,
            args.live_lines
        ),
    );
    row(
        "live",
        "refresco (min / max)",
        format!(
            "{} / {}",
            ms(*times.iter().min().unwrap_or(&total)),
            ms(*times.iter().max().unwrap_or(&total))
        ),
    );
    if let Some((first, last)) = common::refresh_segments(&times) {
        row(
            "live",
            "refresco (mediana primeros 20 / últimos 20)",
            format!("{} / {}", ms(first), ms(last)),
        );
    }
    row("live", "registros nuevos", records);
    Ok(())
}

/// Checkpoints del WAL completados y aplazados por un lector en `refreshes`.
static CHECKPOINTS_DONE: AtomicUsize = AtomicUsize::new(0);
static CHECKPOINTS_BUSY: AtomicUsize = AtomicUsize::new(0);

/// Resumen completo con un lector recién abierto (sin caché de resultados).
fn cold_summary(target: &db::Target) -> Result<Duration> {
    let pool = db::ReadPool::open(target, 1)?;
    let t = Instant::now();
    pool.with(|c| common::full_summary(c, &common::last_30_days()))?;
    Ok(t.elapsed())
}

fn wal_bytes(target: &db::Target) -> Option<u64> {
    match target {
        db::Target::File(path) => std::fs::metadata(format!("{}-wal", path.display()))
            .ok()
            .map(|m| m.len()),
        _ => None,
    }
}

/// Refrescos del escritor sin lecturas y con lecturas a la vez: el Resumen en bucle (`loop`) o
/// un Resumen por aviso, como la interfaz (`ui`). Con la caché en disco no deberían esperarse.
fn contention(
    conn: &mut Connection,
    target: &db::Target,
    providers: &[Box<dyn agentboard_lib::providers::Provider>],
    args: &common::Args,
) -> Result<()> {
    let live = live_file(args)?;
    let (alone, _) = refreshes(conn, providers, args, &live, 0, &|| {}, Some(target))?;
    let wal_alone = wal_bytes(target);
    let ckpt_alone = (
        CHECKPOINTS_DONE.swap(0, Ordering::Relaxed),
        CHECKPOINTS_BUSY.swap(0, Ordering::Relaxed),
    );
    // Tras la tanda sin lecturas, para que antes y después difieran solo en la tanda con lecturas.
    let summary_before = cold_summary(target)?;
    let pool = std::sync::Arc::new(db::ReadPool::open(target, db::ReadPool::SIZE)?);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    // Avisos como `data:changed`: por el notificador real (como mucho uno por segundo).
    let (notice_tx, notice_rx) = std::sync::mpsc::channel::<()>();
    let notifier = agentboard_lib::notifier::spawn(move |_| {
        let _ = notice_tx.send(());
    });
    let reader_kind = args.reader;
    let ui_request = last_30_days_request();
    let reader = {
        let (pool, stop) = (pool.clone(), stop.clone());
        std::thread::spawn(move || -> Result<(usize, Vec<Duration>)> {
            let f = common::last_30_days();
            let mut times = Vec::new();
            loop {
                match reader_kind {
                    Reader::Loop => {
                        if stop.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                    }
                    Reader::Ui => {
                        if notice_rx.recv().is_err() {
                            break;
                        }
                        // Avisos acumulados mientras leía: una sola lectura, como la interfaz.
                        while notice_rx.try_recv().is_ok() {}
                    }
                }
                let t = Instant::now();
                match reader_kind {
                    Reader::Loop => pool.with(|c| common::full_summary(c, &f))?,
                    // La interfaz pide el Resumen en una sola petición.
                    Reader::Ui => pool.with(|c| {
                        agentboard_lib::snapshot::dashboard(c, &ui_request, common::WINDOW_END_MS)
                            .map(|_| 0)
                    })?,
                };
                times.push(t.elapsed());
            }
            Ok((times.len(), times))
        })
    };
    let generation = std::cell::Cell::new(0u64);
    let notify = || {
        generation.set(generation.get() + 1);
        let touched = agentboard_lib::derived::Touched {
            agents: ["claude-code".to_string()].into(),
            ..Default::default()
        };
        notifier.send(generation.get(), touched);
    };
    let busy = refreshes(conn, providers, args, &live, 1, &notify, Some(target));
    drop(notifier);
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let (summaries, summary_times) = reader
        .join()
        .map_err(|_| anyhow::anyhow!("el hilo lector falló"))??;
    let (busy, _) = busy?;
    let summary_after = cold_summary(target)?;
    let label = match args.reader {
        Reader::Loop => "Resumen en bucle",
        Reader::Ui => "un Resumen por aviso (ui)",
    };
    let (a, b) = (median(&alone), median(&busy));
    row("contention", "refresco sin lecturas (mediana)", ms(a));
    row(
        "contention",
        &format!("refresco con {label} (mediana)"),
        ms(b),
    );
    row(
        "contention",
        "con / sin lecturas",
        format!("× {:.2}", b.as_secs_f64() / a.as_secs_f64().max(1e-9)),
    );
    row(
        "contention",
        "Resúmenes completos durante los refrescos",
        format!(
            "{summaries} (mediana {})",
            if summary_times.is_empty() {
                "n/d".to_string()
            } else {
                ms(median(&summary_times))
            }
        ),
    );
    row(
        "contention",
        "checkpoints hechos / aplazados (sin lecturas · con lecturas)",
        format!(
            "{} / {} · {} / {}",
            ckpt_alone.0,
            ckpt_alone.1,
            CHECKPOINTS_DONE.load(Ordering::Relaxed),
            CHECKPOINTS_BUSY.load(Ordering::Relaxed)
        ),
    );
    row(
        "contention",
        "WAL tras la tanda sin lecturas",
        wal_alone.map_or("n/d".to_string(), |b| {
            format!("{:.1} MiB", b as f64 / 1048576.0)
        }),
    );
    row(
        "contention",
        "WAL al terminar",
        match wal_bytes(target) {
            Some(b) => format!("{:.1} MiB", b as f64 / 1048576.0),
            None => "n/d (en memoria)".to_string(),
        },
    );
    row(
        "contention",
        "Resumen en frío antes / después",
        format!(
            "{} / {} (× {:.2})",
            ms(summary_before),
            ms(summary_after),
            summary_after.as_secs_f64() / summary_before.as_secs_f64().max(1e-9)
        ),
    );
    Ok(())
}

// ------------------------------------------------------------------ vigilante (watch, idle)

/// Escritor real (hilo de ingesta) tras su escaneo inicial, con el vigilante y el notificador
/// reales en marcha: `changed` recibe el instante de cada `data:changed` y `scans` las
/// estadísticas de cada relectura del vigilante.
struct LiveApp {
    readers: db::SharedReaders,
    changed: std::sync::mpsc::Receiver<Instant>,
    scans: std::sync::mpsc::Receiver<ingest::ScanStats>,
    _ingest: IngestHandle,
}

fn live_app(args: &common::Args) -> Result<LiveApp> {
    let mode = match &args.cache {
        Some(path) => db::CacheMode::Disco(path.clone()),
        None => db::CacheMode::Memoria,
    };
    let storage = db::Storage::open(mode)?;
    db::set_cache_tz(&storage.writer, common::TZ)?;
    let readers: db::SharedReaders = std::sync::Arc::new(std::sync::RwLock::new(
        std::sync::Arc::new(storage.readers(db::ReadPool::SIZE)?),
    ));
    let (scan_tx, scan_rx) = std::sync::mpsc::channel();
    let (changed_tx, changed_rx) = std::sync::mpsc::channel();
    let (init_tx, init_rx) = std::sync::mpsc::channel();
    let notifier = agentboard_lib::notifier::spawn(move |_| {
        let _ = changed_tx.send(Instant::now());
    });
    let t = Instant::now();
    let ingest = IngestThread::spawn_with_changes(
        storage,
        readers.clone(),
        Vec::new,
        common::providers(&args.fixtures),
        Default::default(),
        move |origen, stats| {
            let _ = match origen {
                Origen::EscaneoInicial => init_tx.send(stats.clone()),
                Origen::Vigilante => scan_tx.send(stats.clone()),
            };
        },
        move |generation, touched| notifier.send(generation, touched.clone()),
    );
    ingest.request(Origen::EscaneoInicial);
    let stats = init_rx
        .recv_timeout(Duration::from_secs(600))
        .context("el escaneo inicial no terminó")?;
    row(
        "setup",
        "escaneo inicial (hilo de ingesta)",
        ms(t.elapsed()),
    );
    row(
        "setup",
        "archivos / líneas / registros",
        format!("{} / {} / {}", stats.files, stats.lines, stats.records),
    );
    let root = args.fixtures.clone();
    watcher::spawn(
        std::sync::Arc::new(move || common::providers(&root)),
        ingest.clone(),
    );
    // Deja que el vigilante registre sus carpetas antes de escribir.
    std::thread::sleep(Duration::from_secs(2));
    while changed_rx.try_recv().is_ok() {}
    while scan_rx.try_recv().is_ok() {}
    Ok(LiveApp {
        readers,
        changed: changed_rx,
        scans: scan_rx,
        _ingest: ingest,
    })
}

fn has_call(readers: &db::SharedReaders, message_id: &str) -> Result<bool> {
    let pool = agentboard_lib::commands::current(readers)?;
    pool.with(|c| {
        // Lo que lee la interfaz al recibir el aviso: el Resumen de todo el historial.
        agentboard_lib::snapshot::dashboard(c, &all_history(), common::WINDOW_END_MS)?;
        Ok(c.query_row(
            "SELECT COUNT(*) FROM calls WHERE message_id = ?1",
            [message_id],
            |r| r.get::<_, i64>(0),
        )? > 0)
    })
}

/// El Resumen de todo el historial, como lo pide la interfaz (con el periodo anterior vacío).
fn all_history() -> agentboard_lib::snapshot::DashboardRequest {
    agentboard_lib::snapshot::DashboardRequest {
        filter: queries::Filter::default(),
        tz: common::TZ.to_string(),
        month_from: common::MONTH_START_MS,
        branch_view: false,
        previous: None,
    }
}

/// El Resumen de los 30 días del filtro fijo, con la comparación con los 30 anteriores.
fn last_30_days_request() -> agentboard_lib::snapshot::DashboardRequest {
    let f = common::last_30_days();
    let previous = f.from.map(|from| queries::Filter {
        from: Some(from - 30 * common::DAY_MS),
        to: Some(from),
        ..f.clone()
    });
    agentboard_lib::snapshot::DashboardRequest {
        filter: f,
        tz: common::TZ.to_string(),
        month_from: common::MONTH_START_MS,
        branch_view: false,
        previous,
    }
}

fn append(path: &std::path::Path, text: &str) -> Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("no se pudo escribir {}", path.display()))?;
    f.write_all(text.as_bytes())?;
    Ok(())
}

const WATCH_WRITES: usize = 20;
const WATCH_SPACING: Duration = Duration::from_millis(1500);
const WATCH_TIMEOUT: Duration = Duration::from_secs(10);

fn percentile(v: &[Duration], p: f64) -> Duration {
    let mut v = v.to_vec();
    v.sort();
    let rank = ((p / 100.0) * v.len() as f64).ceil() as usize;
    v[rank.clamp(1, v.len()) - 1]
}

/// Escrituras aisladas: tiempo desde que `write` vuelve hasta que la lectura pedida con el aviso
/// ya cuenta la llamada nueva.
fn watch_isolated(app: &LiveApp, args: &common::Args) -> Result<()> {
    let (guard, session) = live_file(args)?;
    let mut latencies = Vec::new();
    let mut lost = 0;
    for i in 0..WATCH_WRITES {
        append(&guard.path, &common::live_lines(&session, i, 1))?;
        let written = Instant::now();
        let id = format!("msg_live_{session}_{i}");
        let mut seen = false;
        while let Some(left) = WATCH_TIMEOUT.checked_sub(written.elapsed()) {
            match app.changed.recv_timeout(left) {
                Ok(_) => {
                    if has_call(&app.readers, &id)? {
                        latencies.push(written.elapsed());
                        seen = true;
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        lost += usize::from(!seen);
        if let Some(rest) = WATCH_SPACING.checked_sub(written.elapsed()) {
            std::thread::sleep(rest);
        }
        while app.changed.try_recv().is_ok() {}
    }
    let full = app.scans.try_iter().filter(|s| s.full).count();
    if latencies.is_empty() {
        bail!("ninguna escritura llegó a verse en {WATCH_TIMEOUT:?}");
    }
    row(
        "watch",
        "evento → dato visible (mediana / p95)",
        format!(
            "{} / {} ({} escrituras)",
            ms(median(&latencies)),
            ms(percentile(&latencies, 95.0)),
            WATCH_WRITES
        ),
    );
    row("watch", "escrituras sin aviso visible", lost);
    row("watch", "escaneos completos", full);
    Ok(())
}

const BURST_FILES: usize = 50;
const BURST_SPAN: Duration = Duration::from_secs(3);
const BURST_QUIET: Duration = Duration::from_secs(3);

/// Ráfaga: `events` líneas repartidas en 50 logs durante 3 s; avisos, relecturas y escaneos.
fn watch_burst(app: &LiveApp, args: &common::Args, events: usize) -> Result<()> {
    let dir = args
        .fixtures
        .join("claude_code/-home-dev-Proyectos-proyecto-00");
    std::fs::create_dir_all(&dir)?;
    let pid = std::process::id();
    let files: Vec<(LiveFile, String)> = (0..BURST_FILES)
        .map(|k| {
            let session = format!("perf-burst-{pid}-{k}");
            (LiveFile::new(dir.join(format!("{session}.jsonl"))), session)
        })
        .collect();
    let step = BURST_SPAN / events as u32;
    let start = Instant::now();
    for i in 0..events {
        let (guard, session) = &files[i % BURST_FILES];
        append(&guard.path, &common::live_lines(session, i, 1))?;
        if let Some(rest) = (step * (i as u32 + 1)).checked_sub(start.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    let written = start.elapsed();
    let mut notices = Vec::new();
    while let Ok(at) = app.changed.recv_timeout(BURST_QUIET) {
        notices.push(at);
    }
    let scans: Vec<ingest::ScanStats> = app.scans.try_iter().collect();
    let last_id = {
        let i = events - 1;
        format!("msg_live_{}_{i}", files[i % BURST_FILES].1)
    };
    let visible = has_call(&app.readers, &last_id)?;
    row(
        "watch",
        "ráfaga",
        format!(
            "{events} líneas en {BURST_FILES} logs ({} ms)",
            written.as_millis()
        ),
    );
    row(
        "watch",
        "avisos a la interfaz / relecturas",
        format!("{} / {}", notices.len(), scans.len()),
    );
    row(
        "watch",
        "separación mínima entre avisos",
        common::min_gap(&notices).map_or("n/d".to_string(), ms),
    );
    row(
        "watch",
        "escaneos completos",
        scans.iter().filter(|s| s.full).count(),
    );
    row("watch", "última línea visible", visible);
    Ok(())
}

fn watch(args: &common::Args) -> Result<()> {
    let app = live_app(args)?;
    match args.events {
        Some(events) => watch_burst(&app, args, events),
        None => watch_isolated(&app, args),
    }
}

fn proc_stat() -> String {
    std::fs::read_to_string("/proc/self/stat").unwrap_or_default()
}

/// Reposo: vigilante y escritor en marcha sin escrituras; CPU del proceso y relecturas.
fn idle(args: &common::Args) -> Result<()> {
    let app = live_app(args)?;
    let before = proc_stat();
    std::thread::sleep(Duration::from_secs(args.idle_secs as u64));
    let after = proc_stat();
    let rescans = app.scans.try_iter().count();
    let notices = app.changed.try_iter().count();
    row(
        "idle",
        &format!("CPU en {} s de reposo", args.idle_secs),
        common::cpu_ms(&before, &after).map_or("n/d".to_string(), |c| format!("{c} ms")),
    );
    row(
        "idle",
        "relecturas / avisos",
        format!("{rescans} / {notices}"),
    );
    Ok(())
}

// ------------------------------------------------------------------ bytes por comando

fn json_len<T: serde::Serialize>(v: &T) -> Result<usize> {
    Ok(serde_json::to_vec(v)?.len())
}

/// Bytes JSON de cada respuesta que pide la interfaz con el filtro de 30 días.
fn payloads(conn: &Connection) -> Result<Vec<(String, usize)>> {
    let f = common::last_30_days();
    let mut out = Vec::new();
    out.push((
        "get_dashboard_snapshot".to_string(),
        json_len(&agentboard_lib::snapshot::dashboard(
            conn,
            &last_30_days_request(),
            common::WINDOW_END_MS,
        )?)?,
    ));
    out.push((
        "get_summary".to_string(),
        json_len(&queries::summary(conn, &f, common::WINDOW_END_MS)?)?,
    ));
    for by in [
        "agent",
        "project",
        "model",
        "tool",
        "command",
        "skill",
        "mcp",
        "agent_type",
    ] {
        out.push((
            format!("get_breakdown({by})"),
            json_len(&queries::breakdown(conn, &f, by)?)?,
        ));
    }
    for bucket in ["day", "hour"] {
        out.push((
            format!("get_timeseries({bucket})"),
            json_len(&queries::timeseries(conn, &f, bucket, common::TZ)?)?,
        ));
    }
    for by in ["agent", "model", "project", "tool"] {
        out.push((
            format!("get_timeseries_by({by})"),
            json_len(&queries::timeseries_by(conn, &f, by, common::TZ)?)?,
        ));
    }
    out.push((
        "get_activity".to_string(),
        json_len(&agentboard_lib::insights::activity(conn, &f)?)?,
    ));
    out.push((
        "get_activity_daily".to_string(),
        json_len(&agentboard_lib::insights::activity_daily(
            conn,
            &f,
            common::TZ,
        )?)?,
    ));
    out.push((
        "list_sessions (página de 50)".to_string(),
        json_len(&sessions::session_page(
            conn,
            &f,
            None,
            &sessions::PageRequest::default(),
        )?)?,
    ));
    let projects = sessions::list_projects(conn, &f)?;
    out.push(("list_project_summaries".to_string(), json_len(&projects)?));
    if let Some(p) = projects.iter().max_by_key(|p| p.sessions) {
        out.push((
            "get_project_detail (el mayor)".to_string(),
            json_len(&sessions::project_overview(conn, &p.key, &f, common::TZ)?)?,
        ));
    }
    let biggest: Option<String> = conn
        .query_row(
            "SELECT session_id FROM calls GROUP BY session_id ORDER BY COUNT(*) DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = biggest {
        out.push((
            "get_session_detail (la mayor)".to_string(),
            json_len(&sessions::session_overview(conn, &id)?)?,
        ));
        out.push((
            "list_session_turns (la mayor, página de 30)".to_string(),
            json_len(&sessions::session_turns(
                conn,
                &id,
                &sessions::TurnPage::default(),
            )?)?,
        ));
    }
    Ok(out)
}

fn timed<T>(f: impl Fn() -> Result<T>) -> Result<(Duration, Duration)> {
    let t = Instant::now();
    f()?;
    let first = t.elapsed();
    let mut warm = Vec::with_capacity(WARM_RUNS);
    for _ in 0..WARM_RUNS {
        let t = Instant::now();
        f()?;
        warm.push(t.elapsed());
    }
    Ok((first, median(&warm)))
}

fn queries(conn: &Connection) -> Result<()> {
    let f = common::last_30_days();
    for (name, query) in common::resumen_queries() {
        let (first, warm) = timed(|| query(conn, &f))?;
        row(
            "queries",
            name,
            format!("{} primera, {} mediana", ms(first), ms(warm)),
        );
    }
    let (first, _) = timed(|| common::full_summary(conn, &f))?;
    let cache = agentboard_lib::result_cache::ResultCache::default();
    let generation = agentboard_lib::ingest_thread::DataGeneration::default();
    let (_, repeated) = timed(|| common::full_summary_cached(conn, &cache, &generation, &f))?;
    row("queries", "resumen completo frío (18 llamadas)", ms(first));
    row(
        "queries",
        "resumen completo repetido (mediana, caché de resultados)",
        ms(repeated),
    );
    let req = last_30_days_request();
    let (first, _) =
        timed(|| agentboard_lib::snapshot::dashboard(conn, &req, common::WINDOW_END_MS))?;
    let snapshot_cache = agentboard_lib::result_cache::ResultCache::default();
    let (_, repeated) = timed(|| {
        agentboard_lib::snapshot::cached(
            &snapshot_cache,
            &generation,
            &req,
            common::WINDOW_END_MS,
            || agentboard_lib::snapshot::dashboard(conn, &req, common::WINDOW_END_MS),
        )
    })?;
    row(
        "queries",
        "snapshot del Resumen con comparación, frío",
        ms(first),
    );
    row(
        "queries",
        "snapshot del Resumen repetido (mediana, caché de resultados)",
        ms(repeated),
    );
    let sizes = payloads(conn)?;
    let largest = sizes.iter().map(|(_, b)| *b).max().unwrap_or(0);
    for (name, bytes) in sizes {
        row("bytes", &name, format!("{:.1} KiB", bytes as f64 / 1024.0));
    }
    row(
        "bytes",
        "respuesta mayor (salvo exportar)",
        format!("{:.1} KiB", largest as f64 / 1024.0),
    );
    Ok(())
}

/// Arranque con una caché en disco ya llena: abrirla (validarla), aplicar los precios, tener los
/// datos del Resumen leídos por un lector y, después, el escaneo incremental que no encuentra nada.
fn warm(
    path: &std::path::Path,
    providers: &[Box<dyn agentboard_lib::providers::Provider>],
) -> Result<()> {
    let t = Instant::now();
    let mut storage = db::Storage::open(db::CacheMode::Disco(path.to_path_buf()))?;
    if storage.mode == db::CacheMode::Memoria {
        bail!("no se pudo abrir la caché {}", path.display());
    }
    agentboard_lib::pricing::apply_overrides(&storage.writer, &[])?;
    let readers = storage.readers(db::ReadPool::SIZE)?;
    let opened = t.elapsed();
    readers.with(|c| common::full_summary(c, &common::last_30_days()))?;
    let summary = t.elapsed();
    let scan = Instant::now();
    let stats = ingest::scan_all(&mut storage.writer, providers)?;
    let incremental = scan.elapsed();
    row("warm", "abrir caché y aplicar precios", ms(opened));
    row("warm", "arranque hasta datos del Resumen", ms(summary));
    row(
        "warm",
        "escaneo incremental sin cambios",
        format!(
            "{} ({} archivos, {} líneas)",
            ms(incremental),
            stats.files,
            stats.lines
        ),
    );
    let size: u64 = ["", "-wal", "-shm"]
        .iter()
        .filter_map(|suffix| std::fs::metadata(format!("{}{suffix}", path.display())).ok())
        .map(|m| m.len())
        .sum();
    row(
        "warm",
        "tamaño de la caché",
        format!("{:.1} MiB", size as f64 / 1048576.0),
    );
    if let Some(kib) = common::peak_rss_kib() {
        row(
            "memoria",
            "RSS pico (VmHWM)",
            format!("{:.1} MiB", kib as f64 / 1024.0),
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    // Como en la app, la zona del sistema (la de la caché) es la de la interfaz: así los periodos
    // de días completos leen de los agregados también al reabrir una caché (`--phase warm`).
    std::env::set_var("TZ", common::TZ);
    telemetry::init();
    let mut args = common::parse_args(std::env::args().skip(1))
        .map_err(|e| anyhow::anyhow!("{e}\n{}", common::USAGE))?;
    if !args.fixtures.is_dir() {
        bail!(
            "no existe {}; genera los fixtures con python3 scripts/gen-fixtures.py <tamaño>",
            args.fixtures.display()
        );
    }
    // Rutas absolutas, como las raíces reales: los eventos del vigilante llegan con rutas absolutas.
    args.fixtures = args.fixtures.canonicalize()?;
    let providers = common::providers(&args.fixtures);
    row("fase", "métrica", "valor");
    row("fixtures", "carpeta", args.fixtures.display());
    if args.phase == Phase::Warm {
        let Some(path) = &args.cache else {
            bail!("--phase warm necesita --cache <ruta> con una caché ya llena (--phase cold --cache)");
        };
        return warm(path, &providers);
    }
    match args.phase {
        Phase::Watch => return watch(&args),
        Phase::Idle => return idle(&args),
        _ => {}
    }
    // Escritor: en la caché de `--cache` (en disco, WAL), en memoria compartida si hay que medir
    // lectores aparte, o en una base privada en memoria (como la Fase 1 y el servidor MCP).
    let storage = match &args.cache {
        Some(path) => Some(db::Storage::open(db::CacheMode::Disco(path.clone()))?),
        None if args.phase == Phase::Contention => Some(db::Storage::open(db::CacheMode::Memoria)?),
        None => None,
    };
    let target = storage.as_ref().map(|s| s.target.clone());
    let mut conn = match storage {
        Some(s) => s.writer,
        None => common::open_measure_db()?,
    };
    db::set_cache_tz(&conn, common::TZ)?;
    cold(
        &mut conn,
        &providers,
        matches!(args.phase, Phase::Cold | Phase::All),
    )?;
    if args.fingerprint {
        for line in common::fingerprint(&conn)?.lines() {
            row("huella", "", line);
        }
    }
    if args.query_fingerprint {
        for line in common::query_fingerprint(&conn)?.lines() {
            row("huella-c", "", line);
        }
    }
    if let (Phase::Contention, Some(target)) = (args.phase, &target) {
        contention(&mut conn, target, &providers, &args)?;
    }
    if matches!(args.phase, Phase::Live | Phase::All) {
        live(&mut conn, &providers, &args)?;
    }
    if matches!(args.phase, Phase::Queries | Phase::All) {
        queries(&conn)?;
    }
    match common::peak_rss_kib() {
        Some(kib) => row(
            "memoria",
            "RSS pico (VmHWM)",
            format!("{:.1} MiB", kib as f64 / 1024.0),
        ),
        None => row("memoria", "RSS pico (VmHWM)", "n/d"),
    }
    Ok(())
}
