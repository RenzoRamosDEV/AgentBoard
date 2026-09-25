# Tasks

## 1. Vigilante

- [x] 1.1 Añadir `notify` y un módulo `watcher.rs` que observe las carpetas de logs; verificar con `cargo build`
- [x] 1.2 Debounce de 400 ms y relectura incremental compartiendo la base en memoria; verificar con test de que no duplica
- [x] 1.3 Re-registrar carpetas que aparecen después y emitir `ingest://done`; arrancar el vigilante en `lib.rs`
