//! Aviso `data:changed` a la interfaz: como mucho uno por segundo. El primero tras un segundo sin
//! avisos sale al momento; los que llegan dentro del segundo se agrupan (unión de agentes y días,
//! última generación) y salen al cumplirse. Un hilo propio lo atiende sin despertarse en reposo.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use crate::derived::Touched;
use serde::Serialize;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// Separación mínima entre dos avisos.
pub const INTERVAL: Duration = Duration::from_secs(1);

/// Lo que recibe la interfaz: sin filas de datos. Listas vacías: puede haber cambiado todo.
/// Los días son `YYYYMMDD` en la zona de la caché.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DataChanged {
    pub generation: u64,
    pub agents: Vec<String>,
    pub days: Vec<i64>,
}

/// El throttle, con el reloj como parámetro.
#[derive(Debug, Default)]
pub struct Throttle {
    last_emit: Option<Instant>,
    pending: Option<(u64, Touched)>,
}

impl Throttle {
    /// Un lote confirmado: devuelve el aviso si puede salir ya.
    pub fn on_change(
        &mut self,
        now: Instant,
        generation: u64,
        touched: &Touched,
    ) -> Option<DataChanged> {
        if touched.is_empty() {
            return None;
        }
        let pending = self
            .pending
            .get_or_insert_with(|| (generation, Touched::default()));
        pending.0 = pending.0.max(generation);
        pending.1.merge(touched);
        match self.last_emit {
            Some(at) if now < at + INTERVAL => None,
            _ => self.emit(now),
        }
    }

    /// Cumplido el plazo: el aviso agrupado, si lo hay.
    pub fn on_deadline(&mut self, now: Instant) -> Option<DataChanged> {
        match self.deadline() {
            Some(at) if now >= at => self.emit(now),
            _ => None,
        }
    }

    /// Cuándo puede salir el aviso pendiente; `None` sin pendiente.
    pub fn deadline(&self) -> Option<Instant> {
        self.pending.as_ref()?;
        self.last_emit.map(|t| t + INTERVAL)
    }

    fn emit(&mut self, now: Instant) -> Option<DataChanged> {
        let (generation, touched) = self.pending.take()?;
        self.last_emit = Some(now);
        Some(if touched.all {
            DataChanged {
                generation,
                agents: Vec::new(),
                days: Vec::new(),
            }
        } else {
            DataChanged {
                generation,
                agents: touched.agents.into_iter().collect(),
                days: touched.days.into_iter().collect(),
            }
        })
    }
}

/// Para pasar los lotes confirmados al hilo del aviso.
#[derive(Clone)]
pub struct Notifier(Sender<(u64, Touched)>);

impl Notifier {
    pub fn send(&self, generation: u64, touched: Touched) {
        let _ = self.0.send((generation, touched));
    }
}

/// Lanza el hilo del aviso; `emit` recibe cada aviso que sale.
pub fn spawn(mut emit: impl FnMut(DataChanged) + Send + 'static) -> Notifier {
    let (tx, rx) = mpsc::channel::<(u64, Touched)>();
    std::thread::spawn(move || {
        let mut throttle = Throttle::default();
        loop {
            let received = match throttle.deadline() {
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(at) => rx.recv_timeout(at.saturating_duration_since(Instant::now())),
            };
            let out = match received {
                Ok((generation, touched)) => {
                    throttle.on_change(Instant::now(), generation, &touched)
                }
                Err(RecvTimeoutError::Timeout) => throttle.on_deadline(Instant::now()),
                Err(RecvTimeoutError::Disconnected) => return,
            };
            if let Some(payload) = out {
                emit(payload);
            }
        }
    });
    Notifier(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derived::Touched;
    use std::time::{Duration, Instant};

    fn touched(agents: &[&str], days: &[i64]) -> Touched {
        Touched {
            agents: agents.iter().map(|a| a.to_string()).collect(),
            days: days.iter().copied().collect(),
            all: false,
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn payload(generation: u64, agents: &[&str], days: &[i64]) -> DataChanged {
        DataChanged {
            generation,
            agents: agents.iter().map(|a| a.to_string()).collect(),
            days: days.to_vec(),
        }
    }

    #[test]
    fn el_primer_cambio_sale_al_momento() {
        let t0 = Instant::now();
        let mut t = Throttle::default();
        assert_eq!(t.deadline(), None);
        assert_eq!(
            t.on_change(t0, 3, &touched(&["a"], &[20260901])),
            Some(payload(3, &["a"], &[20260901]))
        );
        assert_eq!(t.deadline(), None, "nada pendiente");
        assert_eq!(t.on_deadline(t0 + ms(5000)), None, "sin cambios no emite");
    }

    #[test]
    fn los_cambios_dentro_del_segundo_se_agrupan_y_salen_al_cumplirse() {
        let t0 = Instant::now();
        let mut t = Throttle::default();
        assert!(t.on_change(t0, 1, &touched(&["a"], &[1])).is_some());
        assert_eq!(t.on_change(t0 + ms(100), 2, &touched(&["b"], &[2])), None);
        assert_eq!(t.on_change(t0 + ms(500), 4, &touched(&["a"], &[3])), None);
        assert_eq!(t.deadline(), Some(t0 + ms(1000)));
        assert_eq!(t.on_deadline(t0 + ms(999)), None);
        assert_eq!(
            t.on_deadline(t0 + ms(1000)),
            Some(payload(4, &["a", "b"], &[2, 3]))
        );
        assert_eq!(t.deadline(), None);
        // El siguiente, dentro del segundo desde el último aviso, espera hasta 2000.
        assert_eq!(t.on_change(t0 + ms(1500), 5, &touched(&["c"], &[9])), None);
        assert_eq!(t.deadline(), Some(t0 + ms(2000)));
        assert_eq!(t.on_deadline(t0 + ms(2001)), Some(payload(5, &["c"], &[9])));
    }

    #[test]
    fn tras_un_segundo_de_calma_vuelve_a_salir_al_momento() {
        let t0 = Instant::now();
        for (gap, immediate) in [(999, false), (1000, true), (1001, true)] {
            let mut t = Throttle::default();
            assert!(t.on_change(t0, 1, &touched(&["a"], &[1])).is_some());
            let got = t.on_change(t0 + ms(gap), 2, &touched(&["a"], &[1]));
            assert_eq!(got.is_some(), immediate, "{gap} ms");
        }
    }

    #[test]
    fn all_vacia_las_listas_y_un_lote_vacio_no_cuenta() {
        let t0 = Instant::now();
        let mut t = Throttle::default();
        assert_eq!(t.on_change(t0, 1, &Touched::default()), None);
        assert_eq!(
            t.deadline(),
            None,
            "un lote sin cambios no deja nada pendiente"
        );
        assert!(t.on_change(t0, 2, &touched(&["a"], &[1])).is_some());
        assert_eq!(t.on_change(t0 + ms(10), 3, &touched(&["b"], &[2])), None);
        assert_eq!(t.on_change(t0 + ms(20), 4, &Touched::everything()), None);
        assert_eq!(t.on_deadline(t0 + ms(1000)), Some(payload(4, &[], &[])));
    }

    #[test]
    fn el_hilo_agrupa_una_rafaga_en_dos_avisos() {
        let (tx, rx) = std::sync::mpsc::channel();
        let notifier = spawn(move |p| tx.send((Instant::now(), p)).unwrap());
        for g in 1..=20 {
            notifier.send(g, touched(&["a"], &[g as i64]));
        }
        let (at1, first) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (at2, second) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(at2 - at1 >= Duration::from_millis(990), "{:?}", at2 - at1);
        assert_eq!(second.generation, 20);
        let mut days: Vec<i64> = first.days.clone();
        days.extend(&second.days);
        days.sort();
        days.dedup();
        assert_eq!(days, (1..=20).collect::<Vec<i64>>());
        assert!(rx.recv_timeout(Duration::from_millis(1500)).is_err());
    }
}
