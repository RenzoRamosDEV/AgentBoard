//! Ingesta completa de la sesión de ~5 MiB del fixture `50m` en una base en memoria nueva.
//! Uso: `cargo bench --bench ingest_file`

mod common;

use agentboard_lib::providers::claude_code::ClaudeCode;
use agentboard_lib::{db, ingest};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use std::path::{Path, PathBuf};

fn big_session(root: &Path) -> Option<PathBuf> {
    ingest::walk(&root.join("claude_code"))
        .into_iter()
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("sesion-grande-"))
        })
}

fn bench(c: &mut Criterion) {
    let Some(root) = common::ensure_fixture("50m") else {
        return;
    };
    let Some(path) = big_session(&root) else {
        eprintln!(
            "AVISO: el fixture 50m no tiene sesion-grande-*.jsonl; regénéralo con {}",
            common::gen_command("50m")
        );
        return;
    };
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let claude_root = root.join("claude_code");
    let mut group = c.benchmark_group("ingest_file");
    group.sample_size(10);
    group.throughput(Throughput::Bytes(bytes));
    group.bench_function("claude_code_5mib", |b| {
        b.iter_batched(
            || {
                (
                    db::open_in_memory().expect("base"),
                    ClaudeCode::with_roots(vec![claude_root.clone()]),
                )
            },
            |(mut conn, provider)| {
                ingest::ingest_file(&mut conn, &provider, &path).expect("ingesta")
            },
            BatchSize::PerIteration,
        )
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
