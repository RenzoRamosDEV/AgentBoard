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
use common::{LiveFile, Phase};
use rusqlite::Connection;
use std::io::Write;
use std::time::{Duration, Instant};

const WARM_RUNS: usize = 5;

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

fn row(phase: &str, metric: &str, value: impl std::fmt::Display) {
    println!("{phase:<10} | {metric:<32} | {value}");
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
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

fn live(
    conn: &mut Connection,
    providers: &[Box<dyn agentboard_lib::providers::Provider>],
    args: &common::Args,
) -> Result<()> {
    let dir = args
        .fixtures
        .join("claude_code/-home-dev-Proyectos-proyecto-00");
    std::fs::create_dir_all(&dir)?;
    let session = format!("perf-live-{}", std::process::id());
    let guard = LiveFile::new(dir.join(format!("{session}.jsonl")));
    let mut times = Vec::with_capacity(args.repeat);
    let mut records = 0;
    for r in 0..args.repeat {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&guard.path)
            .with_context(|| format!("no se pudo escribir {}", guard.path.display()))?;
        f.write_all(common::live_lines(&session, r * args.live_lines, args.live_lines).as_bytes())?;
        drop(f);
        let t = Instant::now();
        let stats = ingest::scan_all(conn, providers)?;
        times.push(t.elapsed());
        records += stats.records;
    }
    let total: Duration = times.iter().sum();
    row(
        "live",
        "refresco (mediana)",
        format!(
            "{} ({} × {} líneas)",
            ms(median(times.clone())),
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
    row("live", "registros nuevos", records);
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
    Ok((first, median(warm)))
}

fn queries(conn: &Connection) -> Result<()> {
    let f = common::last_30_days();
    for (name, query) in common::top_queries() {
        let (first, warm) = timed(|| query(conn, &f))?;
        row(
            "queries",
            name,
            format!("{} primera, {} mediana", ms(first), ms(warm)),
        );
    }
    let (first, warm) = timed(|| common::full_summary(conn, &f))?;
    row(
        "queries",
        "resumen completo (18 llamadas)",
        format!("{} primera, {} mediana", ms(first), ms(warm)),
    );
    Ok(())
}

fn main() -> Result<()> {
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
    let mut conn = db::open_in_memory()?;
    row("fase", "métrica", "valor");
    row("fixtures", "carpeta", args.fixtures.display());
    cold(
        &mut conn,
        &providers,
        matches!(args.phase, Phase::Cold | Phase::All),
    )?;
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
