//! Caché en memoria de los resultados de las consultas del Resumen y de los apartados, válida
//! mientras no cambie la generación de datos (cada `COMMIT` del escritor la sube).

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use crate::ingest_thread::DataGeneration;
use anyhow::Result;
use serde::Serialize;
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Entradas como mucho; al llenarse se vacía entera (más simple que un LRU y el Resumen completo
/// son ~25 entradas por periodo).
pub const CAPACITY: usize = 256;

type Entry = Arc<dyn Any + Send + Sync>;

#[derive(Default)]
struct Inner {
    generation: u64,
    entries: HashMap<String, Entry>,
}

#[derive(Default)]
pub struct ResultCache {
    inner: Mutex<Inner>,
}

/// Clave de una consulta: su tipo y sus argumentos en JSON (los campos en el orden del tipo, no
/// en el del JSON que llegó de la interfaz).
pub fn key(kind: &str, args: &impl Serialize) -> String {
    format!("{kind}|{}", serde_json::to_string(args).unwrap_or_default())
}

/// Clave de una consulta que depende de la hora (el `burn rate` del resumen): cambia cada minuto.
pub fn key_at_minute(kind: &str, args: &impl Serialize, now_ms: i64) -> String {
    format!("{}|{}", key(kind, args), now_ms.div_euclid(60_000))
}

impl ResultCache {
    /// El resultado de `key` en la generación vigente, o el de `compute`, que se guarda solo si
    /// la generación no cambió mientras se calculaba (si no, podría no incluir el lote nuevo).
    pub fn get_or_compute<T: Any + Send + Sync>(
        &self,
        generation: &DataGeneration,
        key: String,
        compute: impl FnOnce() -> Result<T>,
    ) -> Result<Arc<T>> {
        let before = generation.get();
        {
            let mut inner = self.lock();
            if inner.generation != before {
                inner.entries.clear();
                inner.generation = before;
            }
            if let Some(hit) = inner
                .entries
                .get(&key)
                .and_then(|e| e.clone().downcast::<T>().ok())
            {
                return Ok(hit);
            }
        }
        let value = Arc::new(compute()?);
        let mut inner = self.lock();
        if generation.get() == before && inner.generation == before {
            if inner.entries.len() >= CAPACITY {
                inner.entries.clear();
            }
            inner.entries.insert(key, value.clone());
        }
        Ok(value)
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // Una consulta que entró en pánico no deja la caché a medias: se sigue usando.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest_thread::DataGeneration;
    use std::cell::Cell;

    fn compute(counter: &Cell<u32>, value: i64) -> impl FnOnce() -> anyhow::Result<i64> + '_ {
        move || {
            counter.set(counter.get() + 1);
            Ok(value)
        }
    }

    #[test]
    fn la_misma_clave_y_generacion_devuelve_el_mismo_resultado_sin_recalcular() {
        let (cache, generation, n) = (
            ResultCache::default(),
            DataGeneration::default(),
            Cell::new(0),
        );
        let a = cache
            .get_or_compute(&generation, "k".into(), compute(&n, 1))
            .unwrap();
        let b = cache
            .get_or_compute(&generation, "k".into(), compute(&n, 2))
            .unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!((*b, n.get()), (1, 1));
        let c = cache
            .get_or_compute(&generation, "otra".into(), compute(&n, 3))
            .unwrap();
        assert_eq!((*c, n.get()), (3, 2));
    }

    #[test]
    fn otra_generacion_vacia_la_cache() {
        let (cache, generation, n) = (
            ResultCache::default(),
            DataGeneration::default(),
            Cell::new(0),
        );
        cache
            .get_or_compute(&generation, "k".into(), compute(&n, 1))
            .unwrap();
        generation.bump_for_tests();
        let v = cache
            .get_or_compute(&generation, "k".into(), compute(&n, 2))
            .unwrap();
        assert_eq!((*v, n.get()), (2, 2));
        assert_eq!(cache.len(), 1, "solo los de la generación vigente");
    }

    #[test]
    fn un_resultado_calculado_mientras_cambio_la_generacion_no_se_guarda() {
        let (cache, generation, n) = (
            ResultCache::default(),
            DataGeneration::default(),
            Cell::new(0),
        );
        let v = cache
            .get_or_compute(&generation, "k".into(), || {
                generation.bump_for_tests();
                Ok(1)
            })
            .unwrap();
        assert_eq!(*v, 1, "se devuelve igual");
        assert_eq!(cache.len(), 0);
        cache
            .get_or_compute(&generation, "k".into(), compute(&n, 2))
            .unwrap();
        assert_eq!(n.get(), 1, "se recalcula");
    }

    #[test]
    fn las_consultas_que_dependen_de_la_hora_cambian_de_clave_cada_minuto() {
        let f = crate::queries::Filter::default();
        let at = |ms: i64| key_at_minute("summary", &f, ms);
        assert_eq!(at(60_000), at(119_999));
        assert_ne!(at(119_999), at(120_000));
        assert_ne!(key("summary", &f), key("breakdown", &f));
    }

    #[test]
    fn al_pasar_del_tope_se_vacia() {
        let (cache, generation, n) = (
            ResultCache::default(),
            DataGeneration::default(),
            Cell::new(0),
        );
        for i in 0..CAPACITY - 1 {
            cache
                .get_or_compute(&generation, format!("k{i}"), compute(&n, 0))
                .unwrap();
        }
        assert_eq!(cache.len(), 255);
        cache
            .get_or_compute(&generation, "k255".into(), compute(&n, 0))
            .unwrap();
        assert_eq!(cache.len(), 256);
        cache
            .get_or_compute(&generation, "k256".into(), compute(&n, 0))
            .unwrap();
        assert_eq!(cache.len(), 1, "llena: se vacía y guarda el nuevo");
    }

    #[test]
    fn filtros_equivalentes_comparten_clave() {
        let a: crate::queries::Filter =
            serde_json::from_str(r#"{"from":1,"to":2,"agents":["x"],"noProject":false}"#).unwrap();
        let b: crate::queries::Filter =
            serde_json::from_str(r#"{"agents":["x"],"to":2,"from":1}"#).unwrap();
        assert_eq!(key("summary", &a), key("summary", &b));
        let c: crate::queries::Filter = serde_json::from_str(r#"{"from":1,"to":3}"#).unwrap();
        assert_ne!(key("summary", &a), key("summary", &c));
        assert_ne!(
            key("breakdown", &(&a, "model")),
            key("breakdown", &(&a, "project"))
        );
    }
}
