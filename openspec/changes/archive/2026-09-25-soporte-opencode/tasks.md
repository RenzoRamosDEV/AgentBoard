# Tasks

## 1. Backend

- [x] 1.1 Migración `0004` con `calls.cost_reported` y vista que lo usa sin precio; verificar con test
- [x] 1.2 Extender `Provider` con fuente SQLite y `ingest.rs` con cursor por `time_updated`; verificar que la reimportación no duplica
- [x] 1.3 Implementar `providers/opencode.rs` (sesiones, turnos, llamadas, herramientas normalizadas, subagentes); verificar con fixture SQL y `cargo run --example scan` con la base real
