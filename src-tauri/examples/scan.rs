//! Importa el historial real de esta máquina a una base temporal e imprime cada apartado.
//! Uso: `cargo run --example scan [ruta.db]`

use agentburn_lib::{db, ingest, insights, providers, queries};
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

    let f = queries::Filter::default();
    let s = queries::summary(&conn, &f, ingest::now_ms())?;
    println!(
        "\n${:.2} cost  {} calls  {} sessions  {:.1}% cache hit  (ahorro ${:.2}, burn ${:.2}/h)",
        s.cost_usd, s.calls, s.sessions, s.cache_hit * 100.0, s.cache_savings_usd, s.burn_rate_usd_h
    );
    let offset = chrono::Local::now().offset().local_minus_utc() as i64 / 60;
    println!("\nDaily Activity");
    for p in queries::timeseries(&conn, &f, "day", offset)? {
        let d = chrono::DateTime::from_timestamp_millis(p.ts + offset * 60_000).unwrap();
        println!("  {}  ${:>8.2}  {:>5}", d.format("%m-%d"), p.cost_usd, p.calls);
    }
    let a = insights::activity(&conn, &f)?;
    println!("\nBy Activity");
    for r in &a.activities {
        let shot = r.one_shot.map(|v| format!("{:.0}%", v * 100.0)).unwrap_or("-".into());
        println!("  {:<14} ${:>8.2}  {:>4} turns  {:>5}", r.key, r.cost_usd, r.turns, shot);
    }
    for (title, by) in [
        ("By Project", "project"),
        ("By Model", "model"),
        ("Core Tools", "tool"),
        ("Shell Commands", "command"),
        ("Skills & Agents", "skill"),
        ("MCP Servers", "mcp"),
        ("Claude Agent Types", "agent_type"),
    ] {
        println!("\n{title}");
        for r in queries::breakdown(&conn, &f, by)?.iter().take(10) {
            println!(
                "  {:<30} ${:>8.2}  {:>5} calls  {:>3} sess  {:>7.0} overhead",
                r.label, r.cost_usd, r.calls, r.sessions, r.overhead_tokens
            );
        }
    }
    Ok(())
}
