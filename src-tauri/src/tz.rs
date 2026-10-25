//! Días y horas locales con las reglas históricas de la zona (verano/invierno incluidos):
//! aplicar a todas las fechas el desfase de hoy desplaza las cercanas a un cambio de hora.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use chrono::{Datelike, Duration, LocalResult, NaiveDateTime, Offset, TimeZone, Timelike};
use chrono_tz::Tz;

/// Zona IANA pedida; si no existe, la del sistema; y si tampoco, UTC.
pub fn parse(name: &str) -> Tz {
    name.parse().ok().or_else(system).unwrap_or(chrono_tz::UTC)
}

/// Zona IANA del sistema (para el binario MCP, la bandeja y la caché; la UI manda la del
/// navegador).
pub fn system() -> Option<Tz> {
    system_name().parse().ok()
}

/// La de la variable `TZ` si es una zona IANA (como hacen libc y el WebView), si no la del
/// sistema y, si tampoco, UTC.
pub fn system_name() -> String {
    zone_name(
        std::env::var("TZ").ok().as_deref(),
        iana_time_zone::get_timezone().ok(),
    )
}

fn zone_name(tz_env: Option<&str>, detected: Option<String>) -> String {
    tz_env
        .map(|v| v.trim_start_matches(':'))
        .filter(|v| v.parse::<Tz>().is_ok())
        .map(str::to_string)
        .or(detected)
        .unwrap_or_else(|| "UTC".into())
}

/// Instante UTC (ms) de una hora local, resolviendo huecos y repeticiones del cambio de hora.
fn resolve(local: NaiveDateTime, tz: &Tz) -> i64 {
    match tz.from_local_datetime(&local) {
        LocalResult::Single(dt) => dt.timestamp_millis(),
        LocalResult::Ambiguous(first, _) => first.timestamp_millis(),
        // Hora saltada (p. ej. una medianoche inexistente): vale la siguiente.
        LocalResult::None => local
            .checked_add_signed(Duration::hours(1))
            .and_then(|next| tz.from_local_datetime(&next).earliest())
            .map(|dt| dt.timestamp_millis())
            .unwrap_or_default(),
    }
}

/// Fecha y hora local de `ts`; `None` si no se puede representar en la zona.
fn local(ts: i64, tz: &Tz) -> Option<NaiveDateTime> {
    let dt = tz.timestamp_millis_opt(ts).earliest()?;
    dt.naive_utc().checked_add_offset(dt.offset().fix())
}

/// Inicio del día local de `ts`, en epoch ms UTC (`ts` tal cual si no tiene hora local).
pub fn day_start(ts: i64, tz: &Tz) -> i64 {
    local(ts, tz)
        .and_then(|l| l.date().and_hms_opt(0, 0, 0))
        .map(|midnight| resolve(midnight, tz))
        .unwrap_or(ts)
}

/// Día local de `ts` como entero `YYYYMMDD` (el de UTC si no tiene hora local; 0 si tampoco).
pub fn day_number(ts: i64, tz: &Tz) -> i64 {
    local(ts, tz)
        .map(|l| l.date())
        .or_else(|| chrono::DateTime::from_timestamp_millis(ts).map(|d| d.date_naive()))
        .map_or(0, |d| {
            i64::from(d.year()) * 10_000 + i64::from(d.month()) * 100 + i64::from(d.day())
        })
}

/// Inicio de la hora local de `ts`, en epoch ms UTC (`ts` tal cual si no tiene hora local).
pub fn hour_start(ts: i64, tz: &Tz) -> i64 {
    local(ts, tz)
        .and_then(|l| l.with_minute(0))
        .and_then(|d| d.with_second(0))
        .and_then(|d| d.with_nanosecond(0))
        .map(|hour| resolve(hour, tz))
        .unwrap_or(ts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn ms(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0)
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn cada_fecha_usa_su_horario_de_verano_o_invierno() {
        let tz = parse("Europe/Madrid");
        // Enero (UTC+1): las 23:30Z son las 00:30 del 16; su medianoche local es 23:00Z del 15.
        assert_eq!(
            day_start(ms(2026, 1, 15, 23, 30), &tz),
            ms(2026, 1, 15, 23, 0)
        );
        // Julio (UTC+2): las 22:30Z son las 00:30 del 16; su medianoche local es 22:00Z del 15.
        assert_eq!(
            day_start(ms(2026, 7, 15, 22, 30), &tz),
            ms(2026, 7, 15, 22, 0)
        );
        assert_eq!(
            hour_start(ms(2026, 7, 15, 22, 30), &tz),
            ms(2026, 7, 15, 22, 0)
        );
    }

    #[test]
    fn marcas_sin_representacion_local_se_devuelven_sin_truncar() {
        let madrid = parse("Europe/Madrid");
        let extremos = [
            i64::MIN,
            i64::MAX,
            chrono::DateTime::<Utc>::MAX_UTC.timestamp_millis(),
            chrono::DateTime::<Utc>::MIN_UTC.timestamp_millis(),
        ];
        for tz in [madrid, chrono_tz::UTC, parse("Pacific/Kiritimati")] {
            for ts in extremos {
                let d = day_start(ts, &tz);
                let h = hour_start(ts, &tz);
                assert!(d <= ts && h <= ts, "{tz} {ts}: {d} {h}");
            }
        }
        assert_eq!(day_start(i64::MAX, &madrid), i64::MAX);
        assert_eq!(hour_start(i64::MIN, &madrid), i64::MIN);
    }

    #[test]
    fn bordes_de_medianoche_y_cambios_de_hora() {
        let tz = parse("Europe/Madrid");
        // Medianoche local exacta (enero, UTC+1) y un ms antes.
        assert_eq!(
            day_start(ms(2026, 1, 15, 23, 0), &tz),
            ms(2026, 1, 15, 23, 0)
        );
        assert_eq!(
            day_start(ms(2026, 1, 15, 23, 0) - 1, &tz),
            ms(2026, 1, 14, 23, 0)
        );
        // 29 de marzo: a las 02:00 locales se pasa a las 03:00 (01:00Z).
        assert_eq!(
            day_start(ms(2026, 3, 29, 1, 30), &tz),
            ms(2026, 3, 28, 23, 0)
        );
        assert_eq!(
            hour_start(ms(2026, 3, 29, 1, 30), &tz),
            ms(2026, 3, 29, 1, 0)
        );
        assert_eq!(
            hour_start(ms(2026, 3, 29, 0, 59), &tz),
            ms(2026, 3, 29, 0, 0)
        );
        // 25 de octubre: las 02:00–03:00 locales se repiten; vale la primera.
        assert_eq!(
            hour_start(ms(2026, 10, 25, 0, 30), &tz),
            ms(2026, 10, 25, 0, 0)
        );
        assert_eq!(
            hour_start(ms(2026, 10, 25, 1, 30), &tz),
            ms(2026, 10, 25, 0, 0)
        );
        assert_eq!(
            day_start(ms(2026, 10, 25, 12, 0), &tz),
            ms(2026, 10, 24, 22, 0)
        );
    }

    #[test]
    fn numero_de_dia_local_en_bordes_y_cambios_de_hora() {
        let tz = parse("Europe/Madrid");
        for (ts, day) in [
            // Medianoche de invierno (UTC+1): un ms antes es el día anterior.
            (ms(2026, 1, 15, 23, 0) - 1, 20260115),
            (ms(2026, 1, 15, 23, 0), 20260116),
            // Medianoche de verano (UTC+2).
            (ms(2026, 7, 15, 22, 0) - 1, 20260715),
            (ms(2026, 7, 15, 22, 0), 20260716),
            // 29 de marzo: la hora que se salta (02:00–03:00 locales, 01:00Z).
            (ms(2026, 3, 28, 23, 0) - 1, 20260328),
            (ms(2026, 3, 28, 23, 0), 20260329),
            (ms(2026, 3, 29, 0, 59), 20260329),
            (ms(2026, 3, 29, 1, 0), 20260329),
            (ms(2026, 3, 29, 22, 0) - 1, 20260329),
            (ms(2026, 3, 29, 22, 0), 20260330),
            // 25 de octubre: la hora repetida y la medianoche siguiente ya en invierno.
            (ms(2026, 10, 24, 22, 0) - 1, 20261024),
            (ms(2026, 10, 24, 22, 0), 20261025),
            (ms(2026, 10, 25, 0, 30), 20261025),
            (ms(2026, 10, 25, 1, 30), 20261025),
            (ms(2026, 10, 25, 23, 0) - 1, 20261025),
            (ms(2026, 10, 25, 23, 0), 20261026),
        ] {
            assert_eq!(day_number(ts, &tz), day, "{ts}");
            assert_eq!(day_number(day_start(ts, &tz), &tz), day, "{ts}");
        }
        assert_eq!(
            day_number(ms(2026, 1, 15, 23, 0), &chrono_tz::UTC),
            20260115
        );
        assert_eq!(
            day_number(i64::MAX, &tz),
            day_number(i64::MAX, &chrono_tz::UTC)
        );
    }

    #[test]
    fn la_variable_tz_manda_sobre_la_zona_detectada() {
        let detected = || Some("Europe/Madrid".to_string());
        assert_eq!(
            zone_name(Some("America/New_York"), detected()),
            "America/New_York"
        );
        assert_eq!(zone_name(Some(":Asia/Tokyo"), detected()), "Asia/Tokyo");
        assert_eq!(zone_name(Some("no/existe"), detected()), "Europe/Madrid");
        assert_eq!(zone_name(Some(""), detected()), "Europe/Madrid");
        assert_eq!(zone_name(None, detected()), "Europe/Madrid");
        assert_eq!(zone_name(None, None), "UTC");
    }

    #[test]
    fn zona_desconocida_no_revienta() {
        let tz = parse("Zona/Inventada");
        assert!(day_start(0, &tz) <= 0);
    }
}
