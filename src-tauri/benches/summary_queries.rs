//! Las 5 consultas más usadas del Resumen (y la llamada completa) sobre el fixture `50m`, con un
//! filtro fijo de los últimos 30 días de la ventana de los fixtures.
//! Uso: `cargo bench --bench summary_queries`

mod common;

use agentboard_lib::ingest;
use criterion::{criterion_group, criterion_main, Criterion};

fn bench(c: &mut Criterion) {
    let Some(root) = common::ensure_fixture("50m") else {
        return;
    };
    let mut conn = common::open_measure_db().expect("base");
    let stats = ingest::scan_all(&mut conn, &common::providers(&root)).expect("escaneo");
    assert_eq!(stats.errors, 0, "{stats:?}");
    let filter = common::last_30_days();
    let mut group = c.benchmark_group("summary_queries");
    group.sample_size(20);
    for (name, query) in common::top_queries() {
        group.bench_function(name, |b| b.iter(|| query(&conn, &filter).expect(name)));
    }
    group.bench_function("resumen_completo", |b| {
        b.iter(|| common::full_summary(&conn, &filter).expect("resumen"))
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
