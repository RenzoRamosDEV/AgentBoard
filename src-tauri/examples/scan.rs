//! Importa el historial real de esta máquina a una base temporal e imprime el resumen.
//! Uso: `cargo run --example scan [ruta.db]`

use agentburn_lib::{db, ingest, providers, queries};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .map(Into::into)
        .unwrap_or_else(|| std::env::temp_dir().join("agentburn-scan.db"));
    let mut conn = db::open(&path)?;
    let t = Instant::now();
    let stats = ingest::scan_all(&mut conn, &providers::all())?;
    println!("base: {}", path.display());
    println!("escaneo: {stats:?} en {:?}", t.elapsed());
    println!("{:#?}", queries::summary(&conn, &queries::Filter::default(), 0)?);
    Ok(())
}
