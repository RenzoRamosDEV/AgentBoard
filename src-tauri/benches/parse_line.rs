//! Parseo de una línea representativa (respuesta con tokens y uso de herramienta) por proveedor
//! y, con el fixture `50m`, de su archivo más grande entero (incluye las líneas que no producen
//! filas). Uso: `cargo bench --bench parse_line`

mod common;

use agentboard_lib::providers::{
    claude_code::ClaudeCode, codex::Codex, copilot::Copilot, cursor::Cursor, gemini::Gemini,
    Provider,
};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;
use std::path::PathBuf;

struct Case {
    name: &'static str,
    provider: Box<dyn Provider>,
    file: &'static str,
    /// Línea (1-based) del fixture que se mide; las anteriores preparan el estado del proveedor.
    line: usize,
}

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures")
        .join(rel)
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "claude_code",
            provider: Box::new(ClaudeCode::with_roots(vec![fixture("claude_code")])),
            file: "claude_code/-home-dev-Proyectos-demo/0b1c2d3e-sesion-demo.jsonl",
            line: 4,
        },
        Case {
            name: "codex",
            provider: Box::new(Codex::with_roots(vec![fixture("codex/sessions")])),
            file: "codex/sessions/2026/09/24/rollout-2026-09-24T09-00-00-0199a1b2-c3d4-4e5f-8a6b-7c8d9e0f1a2b.jsonl",
            line: 8,
        },
        Case {
            name: "copilot",
            provider: Box::new(Copilot::with_roots(vec![fixture("copilot/session-state")])),
            file: "copilot/session-state/caab1e17-509c-43b4-9ff2-13d1427c36a1/events.jsonl",
            line: 6,
        },
        Case {
            name: "cursor",
            provider: Box::new(Cursor::with_roots(vec![fixture("cursor/projects")])),
            file: "cursor/projects/-home-dev-Proyectos-demo/agent-transcripts/1031d227-0c67-4e17-8954-0b6e2b3322f0/1031d227-0c67-4e17-8954-0b6e2b3322f0.jsonl",
            line: 2,
        },
        Case {
            name: "gemini",
            provider: Box::new(Gemini::with_roots(vec![fixture("gemini/tmp")])),
            file: "gemini/tmp/8f3c/chats/session-2026-09-24T11-00-a1b2c3d4.jsonl",
            line: 3,
        },
    ]
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_line");
    for case in cases() {
        let path = fixture(case.file);
        let text = std::fs::read_to_string(&path).expect("fixture versionado");
        let lines: Vec<&str> = text.lines().collect();
        for prev in &lines[..case.line - 1] {
            let _ = case.provider.parse_line(&path, prev);
        }
        let line = lines[case.line - 1];
        let records = case
            .provider
            .parse_line(&path, line)
            .expect("la línea medida parsea");
        assert!(
            !records.is_empty(),
            "{}: la línea no produce registros",
            case.name
        );
        group.bench_function(case.name, |b| {
            b.iter(|| case.provider.parse_line(black_box(&path), black_box(line)))
        });
    }
    group.finish();
}

/// Archivo más grande del proveedor en el fixture (el primero por ruta si empatan).
fn largest(provider: &dyn Provider) -> Option<PathBuf> {
    provider
        .log_roots()
        .iter()
        .flat_map(|r| agentboard_lib::ingest::walk(r))
        .filter(|p| provider.matches(p))
        .max_by_key(|p| {
            (
                std::fs::metadata(p).map_or(0, |m| m.len()),
                std::cmp::Reverse(p.clone()),
            )
        })
}

fn bench_files(c: &mut Criterion) {
    let Some(root) = common::ensure_fixture("50m") else {
        return;
    };
    let mut group = c.benchmark_group("parse_file");
    group.sample_size(20);
    for (name, provider) in ["claude_code", "codex", "copilot", "cursor", "gemini"]
        .into_iter()
        .zip(common::providers(&root))
    {
        let Some(path) = largest(provider.as_ref()) else {
            continue;
        };
        let text = std::fs::read_to_string(&path).expect("fixture 50m");
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_function(name, |b| {
            b.iter(|| {
                provider.reset(&path);
                for line in text.lines() {
                    let _ = black_box(provider.parse_line(&path, black_box(line)));
                }
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench, bench_files);
criterion_main!(benches);
