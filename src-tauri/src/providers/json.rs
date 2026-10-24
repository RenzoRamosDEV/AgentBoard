//! Tipos de deserialización tolerantes para leer logs sin construir `serde_json::Value`: un
//! campo con un tipo inesperado se trata como ausente (igual que `Value::as_str`, `as_i64` o
//! `as_bool`), las cadenas sin escapes se leen prestadas de la línea y lo que no se declara se
//! salta sin copiarlo.

use serde::de::{self, value::MapAccessDeserializer, Deserialize, Deserializer, IgnoredAny};
use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;

/// Visitante que descarta cualquier valor que no sea del tipo esperado.
macro_rules! ignore_others {
    ($value:ty, $none:expr, [$($skip:ident),*]) => {
        $(ignore_others!(@one $skip, $value, $none);)*
    };
    (@one bool, $value:ty, $none:expr) => {
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<$value, E> { Ok($none) }
    };
    (@one i64, $value:ty, $none:expr) => {
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<$value, E> { Ok($none) }
    };
    (@one u64, $value:ty, $none:expr) => {
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<$value, E> { Ok($none) }
    };
    (@one f64, $value:ty, $none:expr) => {
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<$value, E> { Ok($none) }
    };
    (@one str, $value:ty, $none:expr) => {
        fn visit_str<E: de::Error>(self, _: &str) -> Result<$value, E> { Ok($none) }
    };
    (@one unit, $value:ty, $none:expr) => {
        fn visit_unit<E: de::Error>(self) -> Result<$value, E> { Ok($none) }
        fn visit_none<E: de::Error>(self) -> Result<$value, E> { Ok($none) }
    };
    (@one seq, $value:ty, $none:expr) => {
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<$value, A::Error> {
            while seq.next_element::<IgnoredAny>()?.is_some() {}
            Ok($none)
        }
    };
    (@one map, $value:ty, $none:expr) => {
        fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<$value, A::Error> {
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            Ok($none)
        }
    };
}

/// Cadena si el valor es una cadena (prestada de la línea si no tiene escapes).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Str<'a>(pub Option<Cow<'a, str>>);

impl Str<'_> {
    pub fn as_deref(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Str<'a> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<'a>(PhantomData<&'a ()>);
        impl<'de: 'a, 'a> de::Visitor<'de> for V<'a> {
            type Value = Str<'a>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("cualquier valor JSON")
            }
            fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Str<'a>, E> {
                Ok(Str(Some(Cow::Borrowed(v))))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Str<'a>, E> {
                Ok(Str(Some(Cow::Owned(v.to_owned()))))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Str<'a>, E> {
                Ok(Str(Some(Cow::Owned(v))))
            }
            ignore_others!(Str<'a>, Str(None), [bool, i64, u64, f64, unit, seq, map]);
        }
        d.deserialize_any(V(PhantomData))
    }
}

/// Entero con la semántica de `Value::as_i64`: 0 si no es un entero que quepa en `i64`.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Int(pub i64);

impl<'de> Deserialize<'de> for Int {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = Int;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("cualquier valor JSON")
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Int, E> {
                Ok(Int(v))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Int, E> {
                Ok(Int(i64::try_from(v).unwrap_or(0)))
            }
            ignore_others!(Int, Int(0), [bool, f64, str, unit, seq, map]);
        }
        d.deserialize_any(V)
    }
}

/// Booleano si el valor es un booleano.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Bool(pub Option<bool>);

impl<'de> Deserialize<'de> for Bool {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = Bool;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("cualquier valor JSON")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Bool, E> {
                Ok(Bool(Some(v)))
            }
            ignore_others!(Bool, Bool(None), [i64, u64, f64, str, unit, seq, map]);
        }
        d.deserialize_any(V)
    }
}

/// Objeto `T` si el valor es un objeto; cualquier otro valor se trata como ausente. Los campos
/// de `T` deben ser a su vez tolerantes para que un tipo inesperado no haga fallar la línea.
#[derive(Debug, Clone, PartialEq)]
pub struct Obj<T>(pub Option<T>);

impl<T> Default for Obj<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Obj<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> de::Visitor<'de> for V<T> {
            type Value = Obj<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("cualquier valor JSON")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Obj<T>, A::Error> {
                T::deserialize(MapAccessDeserializer::new(map)).map(|t| Obj(Some(t)))
            }
            ignore_others!(Obj<T>, Obj(None), [bool, i64, u64, f64, str, unit, seq]);
        }
        d.deserialize_any(V(PhantomData))
    }
}

/// Lista de `T` si el valor es una lista; cualquier otro valor, lista vacía.
#[derive(Debug, Clone, PartialEq)]
pub struct List<T>(pub Vec<T>);

impl<T> Default for List<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for List<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> de::Visitor<'de> for V<T> {
            type Value = List<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("cualquier valor JSON")
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<List<T>, A::Error> {
                let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(item) = seq.next_element()? {
                    out.push(item);
                }
                Ok(List(out))
            }
            ignore_others!(
                List<T>,
                List(Vec::new()),
                [bool, i64, u64, f64, str, unit, map]
            );
        }
        d.deserialize_any(V(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, Deserialize)]
    #[serde(default)]
    struct Fields<'a> {
        #[serde(borrow)]
        s: Str<'a>,
        n: Int,
        b: Bool,
        o: Obj<Inner>,
        l: List<Int>,
    }

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default)]
    struct Inner {
        n: Int,
    }

    fn parse(json: &str) -> Fields<'_> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn cada_tipo_lee_su_valor() {
        let f = parse(r#"{"s":"hola","n":-3,"b":true,"o":{"n":4},"l":[1,2]}"#);
        assert!(matches!(f.s.0, Some(Cow::Borrowed("hola"))));
        assert_eq!((f.n, f.b), (Int(-3), Bool(Some(true))));
        assert_eq!(f.o.0, Some(Inner { n: Int(4) }));
        assert_eq!(f.l.0, vec![Int(1), Int(2)]);
    }

    #[test]
    fn las_cadenas_con_escapes_se_copian() {
        let f = parse(r#"{"s":"a\"b"}"#);
        assert!(matches!(f.s.0, Some(Cow::Owned(ref s)) if s == "a\"b"));
    }

    #[test]
    fn un_tipo_inesperado_es_ausente_como_con_value() {
        for other in [
            "1",
            "1.5",
            "true",
            "null",
            "\"x\"",
            "[1,{\"a\":[2]}]",
            "{\"a\":{\"b\":[]}}",
        ] {
            let json =
                format!(r#"{{"s":{other},"n":{other},"b":{other},"o":{other},"l":{other}}}"#);
            let f = parse(&json);
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(f.s.as_deref(), v["s"].as_str(), "{json}");
            assert_eq!(f.n.0, v["n"].as_i64().unwrap_or(0), "{json}");
            assert_eq!(f.b.0, v["b"].as_bool(), "{json}");
            assert_eq!(f.o.0.is_some(), v["o"].is_object(), "{json}");
            assert_eq!(
                f.l.0.is_empty(),
                !v["l"].is_array() || v["l"] == serde_json::json!([])
            );
        }
    }

    #[test]
    fn enteros_en_los_limites_de_i64() {
        assert_eq!(parse(r#"{"n":9223372036854775807}"#).n, Int(i64::MAX));
        assert_eq!(parse(r#"{"n":9223372036854775808}"#).n, Int(0));
        assert_eq!(parse(r#"{"n":-9223372036854775808}"#).n, Int(i64::MIN));
        assert_eq!(parse(r#"{"n":1e3}"#).n, Int(0));
    }
}
