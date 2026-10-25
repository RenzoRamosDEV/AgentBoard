//! Medición de rendimiento sobre una carpeta de fixtures sintéticos (nunca los logs reales):
//! escaneo frío, refresco en vivo, consultas del Resumen y memoria pico.
//!
//! Uso (desde `src-tauri/`):
//!   cargo run --release --example perf_scan -- --fixtures target/perf-fixtures/50m
//!   cargo run --release --example perf_scan -- --fixtures <dir> --phase live --repeat 200

#[path = "../benches/common/mod.rs"]
mod common;

use agentboard_lib::{db, ingest, telemetry};
use anyhow::{bail, Context, Result};
use common::{median, LiveFile, Phase};
use rusqlite::Connection;
use std::io::Write;
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
    guard: &LiveFile,
    session: &str,
    round: usize,
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
    let (guard, session) = live_file(args)?;
    let (times, records) = refreshes(conn, providers, args, &guard, &session, 0)?;
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

/// Refrescos del escritor sin lecturas y con un hilo que repite el Resumen completo por los
/// lectores a la vez: con la caché en disco no deberían esperarse.
fn contention(
    conn: &mut Connection,
    target: &db::Target,
    providers: &[Box<dyn agentboard_lib::providers::Provider>],
    args: &common::Args,
) -> Result<()> {
    let (guard, session) = live_file(args)?;
    let (alone, _) = refreshes(conn, providers, args, &guard, &session, 0)?;
    let pool = std::sync::Arc::new(db::ReadPool::open(target, db::ReadPool::SIZE)?);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader = {
        let (pool, stop) = (pool.clone(), stop.clone());
        std::thread::spawn(move || -> Result<(usize, Vec<Duration>)> {
            let f = common::last_30_days();
            let mut times = Vec::new();
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let t = Instant::now();
                pool.with(|c| common::full_summary(c, &f))?;
                times.push(t.elapsed());
            }
            Ok((times.len(), times))
        })
    };
    let busy = refreshes(conn, providers, args, &guard, &session, 1);
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let (summaries, summary_times) = reader
        .join()
        .map_err(|_| anyhow::anyhow!("el hilo lector falló"))??;
    let (busy, _) = busy?;
    let (a, b) = (median(&alone), median(&busy));
    row("contention", "refresco sin lecturas (mediana)", ms(a));
    row(
        "contention",
        "refresco con el Resumen en bucle (mediana)",
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
    Ok(())
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
    let args = common::parse_args(std::env::args().skip(1))
        .map_err(|e| anyhow::anyhow!("{e}\n{}", common::USAGE))?;
    if !args.fixtures.is_dir() {
        bail!(
            "no existe {}; genera los fixtures con python3 scripts/gen-fixtures.py <tamaño>",
            args.fixtures.display()
        );
    }
    let providers = common::providers(&args.fixtures);
    row("fase", "métrica", "valor");
    row("fixtures", "carpeta", args.fixtures.display());
    if args.phase == Phase::Warm {
        let Some(path) = &args.cache else {
            bail!("--phase warm necesita --cache <ruta> con una caché ya llena (--phase cold --cache)");
        };
        return warm(path, &providers);
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
