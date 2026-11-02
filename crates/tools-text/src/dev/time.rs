//! Time tools: Unix timestamps, date math, time zones and cron.

use crate::util::*;
use chrono::{
    DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone, Utc,
};
use chrono_tz::Tz;
use dogubako_core::prelude::*;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![timestamp(), datetime(), timezone(), cron()]
}

/// Parse many common date/time spellings into a naive (wall-clock) datetime.
/// True for numeric dates whose day/month order is ambiguous, such as
/// `03/04/2024` or `03-04-2024 10:00` (optionally followed by a time).
fn is_ambiguous_numeric_date(s: &str) -> bool {
    let date = s.split_whitespace().next().unwrap_or("");
    let Some(sep) = date.chars().find(|c| *c == '/' || *c == '-') else {
        return false;
    };
    let parts: Vec<&str> = date.split(sep).collect();
    parts.len() == 3
        && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))
        && (1..=2).contains(&parts[0].len())
        && (1..=2).contains(&parts[1].len())
        && parts[2].len() == 4
}

pub(crate) fn parse_loose(s: &str, day_first: bool) -> Option<NaiveDateTime> {
    let s = s.trim();
    if s.len() < 6 || s.len() > 64 {
        return None;
    }
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d.naive_local());
    }
    if let Ok(d) = DateTime::parse_from_rfc2822(s) {
        return Some(d.naive_local());
    }
    const DT: &[&str] = &[
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d %H:%M",
        "%d.%m.%Y %H:%M:%S",
        "%d.%m.%Y %H:%M",
        "%Y%m%dT%H%M%S",
        "%Y%m%dT%H%M%SZ",
        "%b %d, %Y %H:%M:%S",
        "%B %d, %Y %H:%M",
        "%d %b %Y %H:%M:%S",
        "%a %b %e %H:%M:%S %Y",
    ];
    for f in DT {
        if let Ok(d) = NaiveDateTime::parse_from_str(s, f) {
            return Some(d);
        }
    }
    // Offsets / Z on ISO-like strings without seconds etc.
    for f in [
        "%Y-%m-%dT%H:%M:%S%.f%z",
        "%Y-%m-%d %H:%M:%S%.f%z",
        "%Y-%m-%d %H:%M:%S %z",
        "%Y-%m-%dT%H:%M%z",
    ] {
        if let Ok(d) = DateTime::parse_from_str(s, f) {
            return Some(d.naive_local());
        }
    }
    let slash_dt: &[&str] = if day_first {
        &[
            "%d/%m/%Y %H:%M:%S",
            "%d/%m/%Y %H:%M",
            "%m/%d/%Y %H:%M:%S",
            "%m/%d/%Y %H:%M",
        ]
    } else {
        &[
            "%m/%d/%Y %H:%M:%S",
            "%m/%d/%Y %H:%M",
            "%d/%m/%Y %H:%M:%S",
            "%d/%m/%Y %H:%M",
        ]
    };
    for f in slash_dt {
        if let Ok(d) = NaiveDateTime::parse_from_str(s, f) {
            return Some(d);
        }
    }
    let slash: &[&str] = if day_first {
        &["%d/%m/%Y", "%m/%d/%Y", "%d-%m-%Y", "%m-%d-%Y"]
    } else {
        &["%m/%d/%Y", "%d/%m/%Y", "%m-%d-%Y", "%d-%m-%Y"]
    };
    const D: &[&str] = &[
        "%Y-%m-%d",
        "%Y/%m/%d",
        "%Y.%m.%d",
        "%d.%m.%Y",
        "%B %d, %Y",
        "%b %d, %Y",
        "%B %d %Y",
        "%b %d %Y",
        "%d %B %Y",
        "%d %b %Y",
        "%A, %B %d, %Y",
        "%a, %b %d, %Y",
        "%d-%b-%Y",
        "%Y-%j",
    ];
    for f in D.iter().chain(slash.iter()) {
        if let Ok(d) = NaiveDate::parse_from_str(s, f) {
            return Some(d.and_time(NaiveTime::MIN));
        }
    }
    if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(d) = NaiveDate::parse_from_str(s, "%Y%m%d") {
            if (1900..2200).contains(&d.year()) {
                return Some(d.and_time(NaiveTime::MIN));
            }
        }
    }
    None
}

/// Human relative time: "in 3 days", "5 minutes ago".
pub(crate) fn relative(delta_secs: i64) -> String {
    let a = delta_secs.unsigned_abs();
    let (n, unit) = match a {
        0..=44 => {
            return if delta_secs == 0 {
                "now".into()
            } else if delta_secs > 0 {
                "in a few seconds".into()
            } else {
                "a few seconds ago".into()
            }
        }
        45..=3599 => ((a + 30) / 60, "minute"),
        3600..=86_399 => ((a + 1800) / 3600, "hour"),
        86_400..=2_591_999 => ((a + 43_200) / 86_400, "day"),
        2_592_000..=31_535_999 => ((a + 1_296_000) / 2_592_000, "month"),
        _ => ((a + 15_768_000) / 31_536_000, "year"),
    };
    let unit = if n == 1 {
        unit.to_string()
    } else {
        format!("{unit}s")
    };
    if delta_secs > 0 {
        format!("in {n} {unit}")
    } else {
        format!("{n} {unit} ago")
    }
}

fn parse_tz(s: &str, param: &str) -> ToolResult<Tz> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("utc") || s.eq_ignore_ascii_case("z") || s.is_empty() {
        return Ok(Tz::UTC);
    }
    s.parse::<Tz>()
        .or_else(|_| {
            chrono_tz::TZ_VARIANTS
                .iter()
                .find(|t| t.name().eq_ignore_ascii_case(s))
                .copied()
                .ok_or(())
        })
        .map_err(|_| {
            ToolError::invalid(
                param,
                format!("unknown time zone '{s}' (use an IANA name like Europe/Berlin)"),
            )
        })
}

fn tz_param(name: &str, default: &str) -> ParamSpec {
    ParamSpec::text(name)
        .default_str(default)
        .placeholder("Europe/Berlin")
}

/// `[-+]digits[.digits]` × `scale`, exactly (fraction digits beyond the
/// scale's resolution are truncated). `None` for any other syntax.
fn decimal_to_scaled(t: &str, scale: i128) -> Option<i128> {
    let (neg, body) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    if (int.is_empty() && frac.is_empty())
        || !int.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
        || int.len() > 30
    {
        return None;
    }
    let mut v: i128 = if int.is_empty() { 0 } else { int.parse().ok()? };
    v = v.checked_mul(scale)?;
    // Fraction: only as many digits as the scale can represent.
    let digits = scale.to_string().len() - 1;
    let f: String = frac.chars().take(digits).collect();
    if !f.is_empty() {
        let fv: i128 = f.parse().ok()?;
        v = v.checked_add(fv * 10i128.pow((digits - f.len()) as u32))?;
    }
    Some(if neg { -v } else { v })
}

/// Interpret user input as an instant: "now", a Unix number (unit auto-detected) or a date string in `tz`.
pub(crate) fn parse_instant(
    s: &str,
    tz: Tz,
    unit: &str,
    param: &str,
) -> ToolResult<(DateTime<Utc>, &'static str)> {
    let t = s.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("now") {
        return Ok((Utc::now(), "now"));
    }
    if let Ok(n) = t.parse::<f64>() {
        // "NaN" and "inf" parse as floats but are not timestamps (a NaN
        // would otherwise cast to 0, the Unix epoch).
        if !n.is_finite() {
            return Err(ToolError::invalid(
                param,
                format!("'{t}' is not a valid timestamp"),
            ));
        }
        let unit = match unit {
            "auto" => {
                let a = n.abs();
                if a >= 1e17 {
                    "ns"
                } else if a >= 1e14 {
                    "us"
                } else if a >= 1e11 {
                    "ms"
                } else {
                    "s"
                }
            }
            u => u,
        };
        let scale: i128 = match unit {
            "ms" => 1_000_000,
            "us" => 1_000,
            "ns" => 1,
            _ => 1_000_000_000,
        };
        // Exact decimal arithmetic for plain numbers (f64 cannot hold
        // nanosecond timestamps); floating point only for exponent forms.
        let nanos: i128 = match decimal_to_scaled(t, scale) {
            Some(v) => v,
            None => (n * scale as f64) as i128,
        };
        let secs = i64::try_from(nanos.div_euclid(1_000_000_000))
            .map_err(|_| ToolError::invalid(param, "timestamp out of range"))?;
        let ns = nanos.rem_euclid(1_000_000_000) as u32;
        let dt = DateTime::from_timestamp(secs, ns)
            .ok_or_else(|| ToolError::invalid(param, "timestamp out of range"))?;
        return Ok((
            dt,
            match unit {
                "ms" => "milliseconds",
                "us" => "microseconds",
                "ns" => "nanoseconds",
                _ => "seconds",
            },
        ));
    }
    if let Ok(d) = DateTime::parse_from_rfc3339(t) {
        return Ok((d.with_timezone(&Utc), "date"));
    }
    if let Ok(d) = DateTime::parse_from_rfc2822(t) {
        return Ok((d.with_timezone(&Utc), "date"));
    }
    // Explicit offsets always win over the fallback zone, with or without
    // seconds ("2024-01-01T12:00+0200", "...12:00Z").
    let zt = match t.strip_suffix(['Z', 'z']) {
        Some(head) if head.contains(':') => format!("{head}+0000"),
        _ => t.to_string(),
    };
    for f in [
        "%Y-%m-%dT%H:%M:%S%.f%z",
        "%Y-%m-%d %H:%M:%S%.f%z",
        "%Y-%m-%d %H:%M:%S %z",
        "%Y-%m-%dT%H:%M%z",
        "%Y-%m-%d %H:%M%z",
        "%Y-%m-%d %H:%M %z",
    ] {
        if let Ok(d) = DateTime::parse_from_str(&zt, f) {
            return Ok((d.with_timezone(&Utc), "date"));
        }
    }
    let naive = parse_loose(t, false).ok_or_else(|| {
        ToolError::invalid(
            param,
            format!("could not understand '{t}' as a date or timestamp"),
        )
    })?;
    let local = tz.from_local_datetime(&naive).earliest().ok_or_else(|| {
        ToolError::invalid(param, format!("{naive} does not exist in {tz} (DST gap)"))
    })?;
    Ok((local.with_timezone(&Utc), "date"))
}

fn describe_instant(dt: DateTime<Utc>, tz: Tz) -> Value {
    let local = dt.with_timezone(&tz);
    let offset = local.offset().fix().local_minus_utc();
    let now = Utc::now();
    json!({
        "unix_seconds": dt.timestamp(),
        "unix_milliseconds": dt.timestamp_millis(),
        "unix_microseconds": dt.timestamp_micros(),
        "unix_nanoseconds": dt.timestamp_nanos_opt().map(|n| json!(n.to_string())).unwrap_or(Value::Null),
        "iso8601_utc": dt.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
        // RFC 2822 only covers years 0-9999; chrono panics outside them.
        "rfc2822": if (0..=9999).contains(&dt.year()) { json!(dt.to_rfc2822()) } else { Value::Null },
        "local": local.format("%Y-%m-%d %H:%M:%S%.f").to_string(),
        "local_iso8601": local.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, false),
        "time_zone": tz.name(),
        "abbreviation": local.format("%Z").to_string(),
        "utc_offset": format!("{}{:02}:{:02}", if offset < 0 { '-' } else { '+' }, offset.abs() / 3600, (offset.abs() % 3600) / 60),
        "weekday": local.format("%A").to_string(),
        "iso_week": format!("{}-W{:02}", local.iso_week().year(), local.iso_week().week()),
        "day_of_year": local.ordinal(),
        "relative": relative(dt.timestamp() - now.timestamp()),
    })
}

fn timestamp() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.timestamp", "Unix Timestamp Converter", Category::Dev)
        .describe("Convert Unix timestamps (seconds, ms, µs, ns — auto-detected) to ISO 8601 / RFC 2822 dates in UTC and any time zone, and dates back to timestamps")
        .keywords(&["unix time", "epoch", "timestamp", "posix time", "iso 8601", "date to timestamp", "milliseconds", "now"])
        .param(ParamSpec::text("value").label("Timestamp or date").default_str("now").primary().placeholder("1700000000 or 2024-05-01 12:00"))
        .param(ParamSpec::choice("unit", &[("auto", "Auto-detect"), ("s", "Seconds"), ("ms", "Milliseconds"), ("us", "Microseconds"), ("ns", "Nanoseconds")]).default_str("auto"))
        .param(tz_param("timezone", "UTC").label("Time zone (for display and for dates without offset)"))
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let tz = parse_tz(p.str_or("timezone", "UTC"), "timezone")?;
        let (dt, interpreted) =
            parse_instant(p.text("value"), tz, p.str_or("unit", "auto"), "value")?;
        let mut d = describe_instant(dt, tz);
        d["interpreted_as"] = json!(interpreted);
        Ok(ToolOutput::data(d))
    })
}

/// Parse "1y 2mo 3w 4d 5h 6m 7s", "-90m" or ISO 8601 "P1Y2M3DT4H5M6S".
pub(crate) fn parse_duration(s: &str) -> ToolResult<(i32, Duration)> {
    let t = s.trim();
    let bad = || {
        ToolError::invalid(
            "duration",
            format!("could not parse duration '{t}' (try '1d 2h', '-30m' or 'P1DT2H')"),
        )
    };
    let too_big = || ToolError::invalid("duration", format!("duration '{t}' is too large"));
    // Checked accumulation: an oversized amount is an error, never a panic.
    let add_ms = |dur: Duration, ms: f64| -> ToolResult<Duration> {
        if !ms.is_finite() || ms.abs() >= i64::MAX as f64 {
            return Err(too_big());
        }
        Duration::try_milliseconds(ms as i64)
            .and_then(|d| dur.checked_add(&d))
            .ok_or_else(too_big)
    };
    let add_mo = |months: i64, n: f64| -> ToolResult<i64> {
        if !n.is_finite() || n.abs() >= i32::MAX as f64 {
            return Err(too_big());
        }
        months.checked_add(n as i64).ok_or_else(too_big)
    };
    let (neg, body) = match t.strip_prefix('-') {
        Some(r) => (true, r.trim()),
        None => (false, t.trim_start_matches('+').trim()),
    };
    let mut months = 0i64;
    let mut dur = Duration::zero();
    let up = body.to_ascii_uppercase();
    if let Some(iso) = up.strip_prefix('P') {
        let (date, time) = iso.split_once('T').unwrap_or((iso, ""));
        let mut num = String::new();
        for c in date.chars() {
            if c.is_ascii_digit() || c == '.' {
                num.push(c);
                continue;
            }
            let n: f64 = num.parse().map_err(|_| bad())?;
            num.clear();
            match c {
                'Y' => months = add_mo(months, n * 12.0)?,
                'M' => months = add_mo(months, n)?,
                'W' => dur = add_ms(dur, n * 604_800_000.0)?,
                'D' => dur = add_ms(dur, n * 86_400_000.0)?,
                _ => return Err(bad()),
            }
        }
        for c in time.chars() {
            if c.is_ascii_digit() || c == '.' {
                num.push(c);
                continue;
            }
            let n: f64 = num.parse().map_err(|_| bad())?;
            num.clear();
            match c {
                'H' => dur = add_ms(dur, n * 3_600_000.0)?,
                'M' => dur = add_ms(dur, n * 60_000.0)?,
                'S' => dur = add_ms(dur, n * 1000.0)?,
                _ => return Err(bad()),
            }
        }
        if !num.is_empty() {
            return Err(bad());
        }
    } else {
        let re = regex::Regex::new(r"(?i)(\d+(?:\.\d+)?)\s*(years?|y|months?|mo|weeks?|w|days?|d|hours?|hrs?|h|minutes?|mins?|m|seconds?|secs?|s|milliseconds?|ms)\b").unwrap();
        let mut matched = 0;
        for c in re.captures_iter(body) {
            matched += c[0].len();
            let n: f64 = c[1].parse().map_err(|_| bad())?;
            match c[2].to_ascii_lowercase().as_str() {
                "y" | "year" | "years" => months = add_mo(months, n * 12.0)?,
                "mo" | "month" | "months" => months = add_mo(months, n)?,
                "w" | "week" | "weeks" => dur = add_ms(dur, n * 604_800_000.0)?,
                "d" | "day" | "days" => dur = add_ms(dur, n * 86_400_000.0)?,
                "h" | "hr" | "hrs" | "hour" | "hours" => dur = add_ms(dur, n * 3_600_000.0)?,
                "m" | "min" | "mins" | "minute" | "minutes" => dur = add_ms(dur, n * 60_000.0)?,
                "ms" | "millisecond" | "milliseconds" => dur = add_ms(dur, n)?,
                _ => dur = add_ms(dur, n * 1000.0)?,
            }
        }
        let rest: String = body
            .chars()
            .filter(|c| !c.is_whitespace() && *c != ',')
            .collect();
        if matched == 0 || rest.is_empty() {
            return Err(bad());
        }
        let stripped = re.replace_all(body, "");
        if stripped
            .chars()
            .any(|c| !c.is_whitespace() && c != ',' && c != '+')
        {
            return Err(bad());
        }
    }
    let months = i32::try_from(months).map_err(|_| too_big())?;
    Ok(if neg { (-months, -dur) } else { (months, dur) })
}

fn add_months<T: TimeZone>(dt: DateTime<T>, months: i32) -> Option<DateTime<T>> {
    if months >= 0 {
        dt.checked_add_months(chrono::Months::new(months as u32))
    } else {
        dt.checked_sub_months(chrono::Months::new(months.unsigned_abs()))
    }
}

/// Calendar difference in years/months/days + h/m/s.
fn calendar_diff(a: NaiveDateTime, b: NaiveDateTime) -> String {
    let (start, end, sign) = if a <= b { (a, b, "") } else { (b, a, "-") };
    let mut months = (end.year() - start.year()) * 12 + end.month() as i32 - start.month() as i32;
    let mut probe = start
        .checked_add_months(chrono::Months::new(months.max(0) as u32))
        .unwrap_or(start);
    if probe > end {
        months -= 1;
        probe = start
            .checked_add_months(chrono::Months::new(months.max(0) as u32))
            .unwrap_or(start);
    }
    let rem = end - probe;
    let (y, mo) = (months / 12, months % 12);
    let days = rem.num_days();
    let secs = rem.num_seconds() - days * 86_400;
    let mut parts = vec![];
    for (n, u) in [
        (y as i64, "year"),
        (mo as i64, "month"),
        (days, "day"),
        (secs / 3600, "hour"),
        ((secs % 3600) / 60, "minute"),
        (secs % 60, "second"),
    ] {
        if n != 0 {
            parts.push(format!("{n} {u}{}", if n == 1 { "" } else { "s" }));
        }
    }
    if parts.is_empty() {
        "0 seconds".into()
    } else {
        format!("{sign}{}", parts.join(", "))
    }
}

fn datetime() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.datetime", "Date Calculator & Formatter", Category::Dev)
        .describe("Parse dates in many formats, format them with strftime patterns, add or subtract durations, or compute the difference between two dates")
        .keywords(&["date", "time", "strftime", "format date", "add days", "date difference", "days between", "date calculator", "duration"])
        .param(ParamSpec::choice("operation", &[("format", "Parse & format"), ("add", "Add / subtract duration"), ("diff", "Difference between dates")]).default_str("format"))
        .param(ParamSpec::text("date").label("Date").default_str("now").primary().placeholder("2024-02-29 14:30, 1700000000, March 3, 2024"))
        .param(ParamSpec::text("format").label("Output format (strftime)").default_str("%Y-%m-%d %H:%M:%S %Z").show_if("operation", &["format", "add"]).advanced())
        .param(ParamSpec::text("duration").label("Duration").placeholder("1y 2mo 3d 4h, -90m, P1DT12H").show_if("operation", &["add"]))
        .param(ParamSpec::text("date2").label("Second date").default_str("now").show_if("operation", &["diff"]))
        .param(tz_param("timezone", "UTC").label("Time zone"))
        .param(ParamSpec::boolean("day_first").label("Ambiguous dates are day-first (03/04 = 3 April)").advanced())
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let tz = parse_tz(p.str_or("timezone", "UTC"), "timezone")?;
        let day_first = p.bool("day_first");
        let get = |name: &str| -> ToolResult<DateTime<Tz>> {
            let s = p.text(name);
            if day_first {
                if let Some(n) = parse_loose(s, true) {
                    if is_ambiguous_numeric_date(s) {
                        return tz.from_local_datetime(&n).earliest().ok_or_else(|| {
                            ToolError::invalid(name, "time does not exist in this zone (DST gap)")
                        });
                    }
                }
            }
            Ok(parse_instant(s, tz, "auto", name)?.0.with_timezone(&tz))
        };
        let fmt = p.str_or("format", "%Y-%m-%d %H:%M:%S %Z");
        let render = |d: &DateTime<Tz>| -> ToolResult<String> {
            use std::fmt::Write;
            let mut s = String::new();
            write!(s, "{}", d.format(fmt))
                .map_err(|_| ToolError::invalid("format", "invalid strftime format"))?;
            Ok(s)
        };
        match p.str("operation")? {
            "add" => {
                let d = get("date")?;
                let (months, dur) = parse_duration(p.str("duration")?)?;
                let r = add_months(d, months)
                    .and_then(|x| x.checked_add_signed(dur))
                    .ok_or_else(|| ToolError::invalid("duration", "result is out of range"))?;
                let mut out = describe_instant(r.with_timezone(&Utc), tz);
                out["formatted"] = json!(render(&r)?);
                out["start"] = json!(render(&d)?);
                Ok(ToolOutput::text(render(&r)?).with_data(out))
            }
            "diff" => {
                let a = get("date")?;
                let b = get("date2")?;
                let d = b.signed_duration_since(a);
                let secs = d.num_seconds();
                let bd = |x: &DateTime<Tz>| x.date_naive();
                let business = {
                    let (s, e) = if bd(&a) <= bd(&b) {
                        (bd(&a), bd(&b))
                    } else {
                        (bd(&b), bd(&a))
                    };
                    let mut n = 0i64;
                    let mut c = s;
                    let total = (e - s).num_days();
                    if total <= 366 * 200 {
                        while c < e {
                            if c.weekday().num_days_from_monday() < 5 {
                                n += 1;
                            }
                            c = c.succ_opt().unwrap_or(e);
                        }
                        json!(n)
                    } else {
                        Value::Null
                    }
                };
                Ok(ToolOutput::data(json!({
                    "from": render(&a)?,
                    "to": render(&b)?,
                    "human": calendar_diff(a.naive_local(), b.naive_local()),
                    "total_days": (secs as f64 / 86_400.0 * 1000.0).round() / 1000.0,
                    "total_weeks": (secs as f64 / 604_800.0 * 1000.0).round() / 1000.0,
                    "total_hours": (secs as f64 / 3600.0 * 100.0).round() / 100.0,
                    "total_minutes": secs / 60,
                    "total_seconds": secs,
                    "weekdays_between": business,
                })))
            }
            _ => {
                let d = get("date")?;
                let mut out = describe_instant(d.with_timezone(&Utc), tz);
                out["formatted"] = json!(render(&d)?);
                Ok(ToolOutput::text(render(&d)?).with_data(out))
            }
        }
    })
}

fn timezone() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.timezone", "Time Zone Converter", Category::Dev)
        .describe("Convert a date and time between IANA time zones (DST-aware), or search the list of time zones with their current offsets")
        .keywords(&["time zone", "timezone", "convert time", "utc", "gmt", "dst", "world clock", "iana", "meeting planner"])
        .param(ParamSpec::choice("mode", &[("convert", "Convert"), ("list", "List time zones")]).default_str("convert"))
        .param(ParamSpec::text("date").label("Date & time").default_str("now").primary().show_if("mode", &["convert"]))
        .param(tz_param("from", "UTC").label("From zone").show_if("mode", &["convert"]))
        .param(ParamSpec::text("to").label("To zones (comma-separated)").default_str("America/New_York, Europe/London, Asia/Tokyo").show_if("mode", &["convert"]))
        .param(ParamSpec::text("search").label("Filter").show_if("mode", &["list"]))
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let now = Utc::now();
        let fmt_off = |secs: i32| {
            format!(
                "UTC{}{:02}:{:02}",
                if secs < 0 { '-' } else { '+' },
                secs.abs() / 3600,
                (secs.abs() % 3600) / 60
            )
        };
        if p.str("mode")? == "list" {
            let q = p.text("search").to_lowercase();
            let rows: Vec<Vec<Value>> = chrono_tz::TZ_VARIANTS
                .iter()
                .filter(|t| q.is_empty() || t.name().to_lowercase().contains(&q))
                .map(|t| {
                    let l = now.with_timezone(t);
                    vec![
                        json!(t.name()),
                        json!(fmt_off(l.offset().fix().local_minus_utc())),
                        json!(l.format("%Z").to_string()),
                        json!(l.format("%Y-%m-%d %H:%M").to_string()),
                    ]
                })
                .collect();
            return Ok(ToolOutput::data(
                json!({"count": rows.len(), "table": table(&["zone", "offset", "abbreviation", "current time"], rows)}),
            ));
        }
        let from = parse_tz(p.str_or("from", "UTC"), "from")?;
        let (dt, _) = parse_instant(p.text("date"), from, "auto", "date")?;
        let mut zones = vec![from];
        for z in p
            .text("to")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let tz = parse_tz(z, "to")?;
            if !zones.contains(&tz) {
                zones.push(tz);
            }
        }
        let base = dt.with_timezone(&from).naive_local().date();
        let rows: Vec<Vec<Value>> = zones
            .iter()
            .map(|t| {
                let l = dt.with_timezone(t);
                let day = match (l.naive_local().date() - base).num_days() {
                    0 => String::new(),
                    1 => "next day".into(),
                    -1 => "previous day".into(),
                    n => format!("{n:+} days"),
                };
                vec![
                    json!(t.name()),
                    json!(l.format("%Y-%m-%d %H:%M:%S").to_string()),
                    json!(l.format("%Z").to_string()),
                    json!(fmt_off(l.offset().fix().local_minus_utc())),
                    json!(day),
                ]
            })
            .collect();
        Ok(ToolOutput::data(
            json!({"utc": dt.to_rfc3339(), "table": table(&["zone", "local time", "abbreviation", "offset", "day"], rows)}),
        ))
    })
}

fn cron() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.cron", "Cron Expression Explainer", Category::Dev)
        .describe("Explain a cron expression in plain English and list its next run times in any time zone (5 or 6 fields, @daily-style aliases, L/W/# extensions)")
        .keywords(&["cron", "crontab", "schedule", "next run", "cron expression", "explain cron", "job scheduler"])
        .param(ParamSpec::text("expression").label("Cron expression").required().primary().placeholder("*/15 9-17 * * MON-FRI"))
        .param(ParamSpec::integer("count").label("Next runs").default_int(10).range(1.0, 100.0))
        .param(tz_param("timezone", "UTC").label("Time zone"))
        .param(ParamSpec::text("start").label("Start from").default_str("now").advanced())
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        use std::str::FromStr;
        let expr = p.str("expression")?.trim();
        let cron = croner::Cron::from_str(expr)
            .map_err(|e| ToolError::invalid("expression", e.to_string()))?;
        let tz = parse_tz(p.str_or("timezone", "UTC"), "timezone")?;
        let (start, _) = parse_instant(p.str_or("start", "now"), tz, "auto", "start")?;
        let start = start.with_timezone(&tz);
        let n = p.int_or("count", 10).clamp(1, 100) as usize;
        let now = Utc::now().timestamp();
        let runs: Vec<Vec<Value>> = cron
            .iter_after(start)
            .take(n)
            .map(|t| {
                vec![
                    json!(t.format("%Y-%m-%d %H:%M:%S %Z").to_string()),
                    json!(t.format("%A").to_string()),
                    json!(relative(t.timestamp() - now)),
                ]
            })
            .collect();
        if runs.is_empty() {
            return Err(ToolError::invalid(
                "expression",
                "this expression never matches",
            ));
        }
        Ok(ToolOutput::text(cron.describe()).with_data(json!({
            "description": cron.describe(),
            "timezone": tz.name(),
            "table": table(&["next run", "weekday", "relative"], runs),
        })))
    })
}

#[cfg(test)]
mod tests {
    use dogubako_core::testing::TestHarness;
    use serde_json::json;

    #[test]
    fn loose_parsing() {
        use super::parse_loose;
        assert_eq!(
            parse_loose("2024-03-15", false).unwrap().to_string(),
            "2024-03-15 00:00:00"
        );
        assert_eq!(
            parse_loose("03/04/2024", false).unwrap().to_string(),
            "2024-03-04 00:00:00"
        );
        assert_eq!(
            parse_loose("03/04/2024", true).unwrap().to_string(),
            "2024-04-03 00:00:00"
        );
        assert_eq!(
            parse_loose("March 5, 2024", false).unwrap().to_string(),
            "2024-03-05 00:00:00"
        );
        assert_eq!(
            parse_loose("15.03.2024 10:30", false).unwrap().to_string(),
            "2024-03-15 10:30:00"
        );
        assert!(parse_loose("hello", false).is_none());
        assert!(parse_loose("123", false).is_none());
        assert_eq!(
            super::parse_duration("1d 2h").unwrap().1.num_seconds(),
            93_600
        );
        assert_eq!(super::parse_duration("P1M2DT3H").unwrap().0, 1);
        assert!(super::parse_duration("banana").is_err());
        // Non-finite numbers are rejected rather than read as the epoch.
        for bad in ["NaN", "nan", "inf", "-infinity", "1e400"] {
            for unit in ["auto", "ms"] {
                let e = super::parse_instant(bad, chrono_tz::UTC, unit, "value").unwrap_err();
                assert!(
                    e.to_string().contains("not a valid timestamp"),
                    "{bad}: {e}"
                );
            }
        }
        // Oversized amounts are errors, not panics, in every unit and syntax.
        for big in [
            "P999999999999999D",
            "PT99999999999999999999H",
            "P99999999999999999999Y",
            "P9999999999W9999999999W",
            "999999999999999 days",
            "99999999999999999999 ms",
        ] {
            let e = super::parse_duration(big).unwrap_err();
            assert!(e.to_string().contains("too large"), "{big}: {e}");
        }
    }

    #[tokio::test]
    async fn timestamps() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run(
                "dev.timestamp",
                json!({"value": "1700000000", "timezone": "Europe/Berlin"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["iso8601_utc"], "2023-11-14T22:13:20Z");
        // Explicit offsets without seconds are honoured (default zone is UTC).
        for v in [
            "2024-01-01T12:00+0200",
            "2024-01-01 12:00+02:00",
            "2024-01-01T10:00Z",
        ] {
            let out = h.run("dev.timestamp", json!({"value": v})).await.unwrap();
            assert_eq!(out.data["iso8601_utc"], "2024-01-01T10:00:00Z", "{v}");
        }
        assert_eq!(out.data["local"], "2023-11-14 23:13:20");
        assert_eq!(out.data["utc_offset"], "+01:00");
        let out = h
            .run("dev.timestamp", json!({"value": "1700000000123"}))
            .await
            .unwrap();
        assert_eq!(out.data["interpreted_as"], "milliseconds");
        assert_eq!(out.data["unix_milliseconds"], 1700000000123i64);
        // Nanosecond and fractional inputs are exact (no f64 rounding).
        let out = h
            .run(
                "dev.timestamp",
                json!({"value": "1700000000000000001", "unit": "ns"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["unix_nanoseconds"], "1700000000000000001");
        let out = h
            .run(
                "dev.timestamp",
                json!({"value": "1700000000.123456789", "unit": "s"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["unix_nanoseconds"], "1700000000123456789");
        let out = h
            .run("dev.timestamp", json!({"value": "-1.5", "unit": "s"}))
            .await
            .unwrap();
        assert_eq!(out.data["iso8601_utc"], "1969-12-31T23:59:58.500Z");
        let out = h
            .run(
                "dev.timestamp",
                json!({"value": "2024-01-01 00:00", "timezone": "America/New_York"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["unix_seconds"], 1704085200);
        assert_eq!(
            h.run("dev.timestamp", json!({"value": "gibberish"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        // Years past 9999 are valid instants without an RFC 2822 form.
        let out = h
            .run(
                "dev.timestamp",
                json!({"value": "253402300800", "unit": "s"}),
            )
            .await
            .unwrap();
        assert!(out.data["rfc2822"].is_null());
        assert!(out.data["iso8601_utc"].as_str().unwrap().contains("10000"));
        let out = h
            .run(
                "dev.datetime",
                json!({"operation": "add", "date": "9999-12-31", "duration": "1d"}),
            )
            .await
            .unwrap();
        assert!(out.text.unwrap().contains("10000"));
        // Seconds beyond i64 are an error, not a wrapped 1970 date.
        let e = h
            .run(
                "dev.timestamp",
                json!({"value": "18446744073709551616", "unit": "s"}),
            )
            .await
            .unwrap_err();
        assert!(e.to_string().contains("out of range"), "{e}");
        assert_eq!(
            h.run("dev.timestamp", json!({"timezone": "Mars/Base"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn date_math() {
        let h = TestHarness::new(crate::tools());
        let out = h.run("dev.datetime", json!({"operation": "add", "date": "2024-01-31", "duration": "1mo 1d", "format": "%Y-%m-%d"})).await.unwrap();
        assert_eq!(out.text.unwrap(), "2024-03-01");
        // Day-first applies to hyphen-separated dates as well as slashes.
        for d in ["03/04/2024", "03-04-2024"] {
            let out = h
                .run(
                    "dev.datetime",
                    json!({"date": d, "day_first": true, "format": "%Y-%m-%d"}),
                )
                .await
                .unwrap();
            assert_eq!(out.text.unwrap(), "2024-04-03", "{d}");
        }
        assert!(super::is_ambiguous_numeric_date("3-4-2024 10:00"));
        assert!(!super::is_ambiguous_numeric_date(
            "2024-03-04T10:00:00+02:00"
        ));
        let out = h
            .run(
                "dev.datetime",
                json!({"operation": "diff", "date": "2024-01-01", "date2": "2025-03-04 06:00"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["human"], "1 year, 2 months, 3 days, 6 hours");
        let out = h
            .run(
                "dev.datetime",
                json!({"operation": "diff", "date": "2024-06-03", "date2": "2024-06-10"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["weekdays_between"], 5);
        let out = h
            .run(
                "dev.datetime",
                json!({"date": "2024-07-04T12:00:00Z", "format": "%A %d %B %Y"}),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "Thursday 04 July 2024");
        assert_eq!(
            h.run(
                "dev.datetime",
                json!({"operation": "add", "duration": "soon"})
            )
            .await
            .unwrap_err()
            .code(),
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn zones_and_cron() {
        let h = TestHarness::new(crate::tools());
        let out = h.run("dev.timezone", json!({"date": "2024-07-01 09:00", "from": "Europe/Berlin", "to": "America/Los_Angeles, Asia/Tokyo"})).await.unwrap();
        let rows = out.data["table"]["rows"].as_array().unwrap();
        assert_eq!(rows[1][1], "2024-07-01 00:00:00");
        assert_eq!(rows[2][1], "2024-07-01 16:00:00");
        let out = h
            .run("dev.timezone", json!({"mode": "list", "search": "berlin"}))
            .await
            .unwrap();
        assert_eq!(out.data["count"], 1);
        assert_eq!(
            h.run("dev.timezone", json!({"to": "Nowhere/City"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        let out = h
            .run(
                "dev.cron",
                json!({"expression": "30 9 * * MON", "count": 3, "start": "2024-01-01T00:00:00Z"}),
            )
            .await
            .unwrap();
        let rows = out.data["table"]["rows"].as_array().unwrap();
        assert_eq!(rows[0][0], "2024-01-01 09:30:00 UTC");
        assert_eq!(rows[1][0], "2024-01-08 09:30:00 UTC");
        assert!(!out.text.unwrap().is_empty());
        assert_eq!(
            h.run("dev.cron", json!({"expression": "61 * * * *"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }
}
