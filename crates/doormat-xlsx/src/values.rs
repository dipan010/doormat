//! Cell values as text, exactly as openpyxl 3.1 reads them and Python's
//! `str()` prints them: number parsing (`int()` / `float()`), float `repr`,
//! and Excel date serials converted to datetimes, times and durations.

/// A number cell as openpyxl casts it: `float` when the text holds `.`, `e`
/// or `E`, otherwise `int`.
#[derive(Debug, Clone, PartialEq)]
pub enum Number {
    /// An integer that fits in `i128`.
    Int(i128),
    /// An integer too large for `i128`, kept as canonical decimal text.
    BigInt(String),
    /// A float.
    Float(f64),
}

/// The canonical decimal digits of Python `int(text)`, or `None` where
/// Python raises `ValueError`. Surrounding whitespace, a sign, leading zeros
/// and single underscores between digits are accepted.
pub fn python_int_digits(text: &str) -> Option<(bool, String)> {
    let t = text.trim();
    let (negative, body) = match t.as_bytes().first()? {
        b'-' => (true, &t[1..]),
        b'+' => (false, &t[1..]),
        _ => (false, t),
    };
    let digits = strip_underscores(body)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let canonical = digits.trim_start_matches('0');
    let canonical = if canonical.is_empty() { "0" } else { canonical };
    Some((negative && canonical != "0", canonical.to_string()))
}

/// Python `int(text)` as a [`Number`], or `None` where Python raises.
pub fn python_int(text: &str) -> Option<Number> {
    let (negative, digits) = python_int_digits(text)?;
    let signed = if negative {
        format!("-{digits}")
    } else {
        digits
    };
    Some(match signed.parse::<i128>() {
        Ok(v) => Number::Int(v),
        Err(_) => Number::BigInt(signed),
    })
}

/// Python `float(text)`, or `None` where Python raises `ValueError`.
pub fn python_float(text: &str) -> Option<f64> {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();
    let unsigned = lower.trim_start_matches(['+', '-']);
    if lower.len() - unsigned.len() > 1 {
        return None;
    }
    if matches!(unsigned, "inf" | "infinity" | "nan") {
        return t.parse::<f64>().ok();
    }
    let cleaned = strip_underscores(t)?;
    // Rust also accepts these spellings; Python does not.
    if cleaned.is_empty() || cleaned.contains(['i', 'I', 'n', 'N']) {
        return None;
    }
    cleaned.parse::<f64>().ok()
}

/// Remove underscores that sit between two digits; `None` for any other underscore.
fn strip_underscores(text: &str) -> Option<String> {
    if !text.contains('_') {
        return Some(text.to_string());
    }
    let bytes = text.as_bytes();
    let ok = bytes.iter().enumerate().all(|(i, &b)| {
        b != b'_'
            || (i > 0
                && i + 1 < bytes.len()
                && bytes[i - 1].is_ascii_digit()
                && bytes[i + 1].is_ascii_digit())
    });
    ok.then(|| text.replace('_', ""))
}

/// openpyxl's `_cast_number`: `float` if the text holds `.`, `e` or `E`, else `int`.
pub fn cast_number(text: &str) -> Option<Number> {
    if text.contains(['.', 'e', 'E']) {
        python_float(text).map(Number::Float)
    } else {
        python_int(text)
    }
}

/// Python `repr(float)`: the shortest round-tripping digits, in exponent
/// form when the decimal point would fall more than 4 places left or 16
/// places right of the first digit.
pub fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let (digits, exp) = shortest_digits(x.abs());
    let sign = if x.is_sign_negative() { "-" } else { "" };
    let decpt = exp + 1;
    let n = digits.len() as i32;
    let body = if decpt <= -4 || decpt > 16 {
        let (first, rest) = digits.split_at(1);
        let frac = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        let e = decpt - 1;
        format!(
            "{first}{frac}e{}{:02}",
            if e < 0 { '-' } else { '+' },
            e.abs()
        )
    } else if decpt <= 0 {
        format!("0.{}{digits}", "0".repeat((-decpt) as usize))
    } else if decpt >= n {
        format!("{digits}{}.0", "0".repeat((decpt - n) as usize))
    } else {
        let (int, frac) = digits.split_at(decpt as usize);
        format!("{int}.{frac}")
    };
    format!("{sign}{body}")
}

/// The shortest digits that round-trip to `x` (with the decimal exponent of
/// the first digit), choosing the one nearest `x` and the even digit on a
/// tie, as Python's `repr` does. Rust's shortest formatter rounds such ties
/// up, so the correctly rounded string of the same length wins when it also
/// round-trips.
fn shortest_digits(x: f64) -> (String, i32) {
    let split = |sci: &str| -> (String, i32) {
        let (mantissa, exp) = sci
            .split_once('e')
            .expect("LowerExp always has an exponent");
        let digits = mantissa.chars().filter(|c| *c != '.').collect();
        (
            digits,
            exp.parse().expect("LowerExp exponent is an integer"),
        )
    };
    let shortest = split(&format!("{x:e}"));
    let n = shortest.0.len();
    let nearest = format!("{x:.*e}", n - 1);
    if nearest.parse::<f64>() == Ok(x) {
        split(&nearest)
    } else {
        shortest
    }
}

/// Python `str()` of a number cell that is not date-formatted.
pub fn number_text(n: &Number) -> String {
    match n {
        Number::Int(v) => v.to_string(),
        Number::BigInt(s) => s.clone(),
        Number::Float(f) => float_repr(*f),
    }
}

const US_PER_DAY: i128 = 86_400_000_000;
const MAX_DELTA_DAYS: i128 = 999_999_999;
/// Days from 0001-01-01 to the 1900 (1899-12-30) and 1904 epochs.
const EPOCH_1900: i128 = 693_593;
const EPOCH_1904: i128 = 695_055;
/// Days from 0001-01-01 to 9999-12-31.
const MAX_ORDINAL: i128 = 3_652_058;

/// Python's `divmod(x, 1.0)` for floats (CPython `float_divmod`).
fn float_divmod_one(x: f64) -> (f64, f64) {
    let mut m = x % 1.0;
    let mut div = x - m;
    if m != 0.0 {
        if m < 0.0 {
            m += 1.0;
            div -= 1.0;
        }
    } else {
        m = 0.0_f64.copysign(1.0);
    }
    let floordiv = if div != 0.0 {
        let mut f = div.floor();
        if div - f > 0.5 {
            f += 1.0;
        }
        f
    } else {
        0.0_f64.copysign(x)
    };
    (floordiv, m)
}

/// Python `timedelta(days=x)` in microseconds, or `None` on OverflowError/ValueError.
fn timedelta_days_us(x: &Number) -> Option<i128> {
    let us = match x {
        Number::Int(d) => d.checked_mul(US_PER_DAY)?,
        Number::BigInt(_) => return None,
        Number::Float(f) => {
            if !f.is_finite() {
                return None;
            }
            // CPython delta_new/accum: exact integral part, then the
            // fractional microseconds rounded half to even.
            let int_part = f.trunc();
            if int_part.abs() > 1e30 {
                return None;
            }
            let whole = (int_part as i128).checked_mul(US_PER_DAY)?;
            let frac = f - int_part;
            if frac == 0.0 {
                whole
            } else {
                let dnum = US_PER_DAY as f64 * frac;
                let int_us = dnum.trunc();
                let leftover = dnum - int_us;
                let x = whole + int_us as i128;
                let mut rounded = leftover.round();
                if (rounded - leftover).abs() == 0.5 {
                    let odd = (x & 1) as f64;
                    rounded = 2.0 * ((leftover + odd) * 0.5).round() - odd;
                }
                x + rounded as i128
            }
        }
    };
    delta_in_range(us).then_some(us)
}

/// True if a duration in microseconds fits Python's `timedelta` (|days| <= 999,999,999).
fn delta_in_range(us: i128) -> bool {
    us.div_euclid(US_PER_DAY).abs() <= MAX_DELTA_DAYS
}

/// Gregorian date from days since 0001-01-01 (day 0).
fn civil_from_ordinal(days: i128) -> (i128, i128, i128) {
    // Howard Hinnant's algorithm, shifted from 1970-01-01 to 0001-01-01.
    let z = days - 719_162 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn clock(us_of_day: i128) -> String {
    let secs = us_of_day / 1_000_000;
    let us = us_of_day % 1_000_000;
    let base = format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60);
    if us != 0 {
        format!("{base}.{us:06}")
    } else {
        base
    }
}

/// openpyxl `from_excel(value, epoch)` printed with `str()`: a datetime, or
/// a time for serials in `[0, 1)`. `None` where openpyxl falls back to
/// `#VALUE!` (the date is out of range).
pub fn excel_datetime_text(value: &Number, date1904: bool) -> Option<String> {
    let (day, fraction_ms, as_float) = match value {
        Number::Int(v) => (*v, 0_i128, *v as f64),
        Number::BigInt(_) => return None,
        Number::Float(f) => {
            if !f.is_finite() {
                return None;
            }
            let (day, fraction) = float_divmod_one(*f);
            let ms = (fraction * 86_400.0 * 1000.0).round_ties_even();
            if day.abs() > 1e30 {
                return None;
            }
            (day as i128, ms as i128, *f)
        }
    };
    let diff_us = fraction_ms * 1000;
    if (0.0..1.0).contains(&as_float) && diff_us < US_PER_DAY {
        return Some(clock(diff_us));
    }
    let day = if !date1904 && 0.0 < as_float && as_float < 60.0 {
        day + 1
    } else {
        day
    };
    if day.abs() > MAX_DELTA_DAYS {
        return None;
    }
    let epoch = if date1904 { EPOCH_1904 } else { EPOCH_1900 };
    let base = epoch + day;
    if !(0..=MAX_ORDINAL).contains(&base) {
        return None;
    }
    let total = base * US_PER_DAY + diff_us;
    let ordinal = total.div_euclid(US_PER_DAY);
    if !(0..=MAX_ORDINAL).contains(&ordinal) {
        return None;
    }
    let (y, m, d) = civil_from_ordinal(ordinal);
    Some(format!(
        "{y:04}-{m:02}-{d:02} {}",
        clock(total.rem_euclid(US_PER_DAY))
    ))
}

/// openpyxl `from_excel(value, timedelta=True)` printed with `str()`, or
/// `None` where openpyxl falls back to `#VALUE!`.
pub fn excel_timedelta_text(value: &Number) -> Option<String> {
    let mut us = timedelta_days_us(value)?;
    if us.rem_euclid(1_000_000) != 0 {
        // timedelta(seconds=td.total_seconds() // 1, microseconds=round(us, -3))
        let total_seconds = if us.abs() < (1_i128 << 53) {
            us as f64 / 1e6
        } else {
            (us / 1_000_000) as f64
        };
        let seconds = total_seconds.floor() as i128;
        let micro = us.rem_euclid(1_000_000);
        let (q, r) = (micro / 1000, micro % 1000);
        let thousands = if r > 500 || (r == 500 && q % 2 == 1) {
            q + 1
        } else {
            q
        };
        us = seconds.checked_mul(1_000_000)? + thousands * 1000;
        if !delta_in_range(us) {
            return None;
        }
    }
    Some(delta_text(us))
}

/// Python `str(timedelta)`: `[D day[s], ]H:MM:SS[.ffffff]`, hours unpadded.
fn delta_text(us: i128) -> String {
    let days = us.div_euclid(US_PER_DAY);
    let of_day = us.rem_euclid(US_PER_DAY);
    let secs = of_day / 1_000_000;
    let micro = of_day % 1_000_000;
    let mut text = format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60);
    if micro != 0 {
        text.push_str(&format!(".{micro:06}"));
    }
    if days != 0 {
        let plural = if days.abs() != 1 { "s" } else { "" };
        text = format!("{days} day{plural}, {text}");
    }
    text
}

/// openpyxl `from_ISO8601(text)` printed with `str()`, for cells of type
/// `d`. `None` where openpyxl raises `ValueError`.
pub fn iso8601_text(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let digits = |s: usize, n: usize| -> Option<i128> {
        let part = b.get(s..s + n)?;
        part.iter().all(u8::is_ascii_digit).then(|| {
            part.iter()
                .fold(0_i128, |acc, d| acc * 10 + i128::from(d - b'0'))
        })
    };
    let mut pos = 0;
    let date = match (digits(0, 4), b.get(4), digits(5, 2), b.get(7), digits(8, 2)) {
        (Some(y), Some(b'-'), Some(m), Some(b'-'), Some(d)) => {
            pos = 10;
            Some((y, m, d))
        }
        _ => None,
    };
    if b.get(pos) == Some(&b'T') {
        pos += 1;
    }
    let mut time = None;
    if let (Some(h), Some(b':'), Some(mi)) = (digits(pos, 2), b.get(pos + 2), digits(pos + 3, 2)) {
        pos += 5;
        let mut sec = 0;
        let mut micro = 0;
        if let (Some(b':'), Some(s)) = (b.get(pos), digits(pos + 1, 2)) {
            sec = s;
            pos += 3;
            if b.get(pos) == Some(&b'.') {
                let n = b[pos + 1..]
                    .iter()
                    .take(3)
                    .take_while(|c| c.is_ascii_digit())
                    .count();
                if n > 0 {
                    let frac = std::str::from_utf8(&b[pos..pos + 1 + n]).ok()?;
                    micro = (frac.parse::<f64>().ok()? * 1_000_000.0) as i128;
                }
            }
        }
        time = Some((h, mi, sec, micro));
    }
    let valid_clock = |h: i128, mi: i128, s: i128| h < 24 && mi < 60 && s < 60;
    match (date, time) {
        (None, None) => None,
        (None, Some((h, mi, s, us))) => {
            valid_clock(h, mi, s).then(|| clock(((h * 60 + mi) * 60 + s) * 1_000_000 + us))
        }
        (Some((y, m, d)), t) => {
            if !(1..=9999).contains(&y) || !(1..=12).contains(&m) || d < 1 || d > month_len(y, m) {
                return None;
            }
            let date = format!("{y:04}-{m:02}-{d:02}");
            match t {
                None => Some(date),
                Some((h, mi, s, us)) => valid_clock(h, mi, s).then(|| {
                    format!(
                        "{date} {}",
                        clock(((h * 60 + mi) * 60 + s) * 1_000_000 + us)
                    )
                }),
            }
        }
    }
}

fn month_len(y: i128, m: i128) -> i128 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse Python's `float.hex()` output (`-0x1.8000000000000p+1`, or
    /// `0x0.0000000000001p-1022` for subnormals) into the exact double.
    fn from_hex(text: &str) -> f64 {
        let (sign, rest) = match text.strip_prefix('-') {
            Some(r) => (1_u64 << 63, r),
            None => (0, text),
        };
        let rest = rest.strip_prefix("0x").expect("hex float");
        let (mantissa, exp) = rest.split_once('p').expect("hex exponent");
        let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let frac = format!("{frac:0<13}");
        let fraction = u64::from_str_radix(&frac, 16).unwrap();
        let exp: i64 = exp.parse().unwrap();
        let biased = match int {
            "0" => 0,
            "1" => (exp + 1023) as u64,
            other => panic!("unexpected hex mantissa {other}"),
        };
        f64::from_bits(sign | (biased << 52) | fraction)
    }

    #[test]
    fn python_vectors() {
        let data = include_str!("../tests/data/value_vectors.tsv");
        let mut checked = 0;
        for line in data.lines() {
            let mut parts = line.splitn(3, '\t');
            let (kind, input, want) = (
                parts.next().unwrap(),
                parts.next().unwrap(),
                parts.next().unwrap(),
            );
            let x = from_hex(input);
            let got = match kind {
                "repr" => Some(float_repr(x)),
                "date" => excel_datetime_text(&Number::Float(x), false),
                "date1904" => excel_datetime_text(&Number::Float(x), true),
                "delta" => excel_timedelta_text(&Number::Float(x)),
                other => panic!("unknown vector kind {other}"),
            };
            assert_eq!(got.as_deref().unwrap_or("!"), want, "{kind} {input}");
            checked += 1;
        }
        assert!(checked > 10_000);
    }

    #[test]
    fn int_parsing_follows_python() {
        assert_eq!(python_int(" 007 "), Some(Number::Int(7)));
        assert_eq!(python_int("-0"), Some(Number::Int(0)));
        assert_eq!(python_int("+1_000"), Some(Number::Int(1000)));
        assert_eq!(python_int("1__0"), None);
        assert_eq!(python_int("_1"), None);
        assert_eq!(python_int(""), None);
        assert_eq!(python_int("1.0"), None);
        assert_eq!(
            python_int("123456789012345678901234567890123456789012"),
            Some(Number::BigInt(
                "123456789012345678901234567890123456789012".into()
            ))
        );
        assert_eq!(python_int_digits("-12"), Some((true, "12".into())));
    }

    #[test]
    fn float_parsing_follows_python() {
        assert_eq!(python_float(" 1.5 "), Some(1.5));
        assert_eq!(python_float(".5"), Some(0.5));
        assert_eq!(python_float("5."), Some(5.0));
        assert_eq!(python_float("1_0.5"), Some(10.5));
        assert_eq!(python_float("1E5"), Some(100_000.0));
        assert!(python_float("Infinity").unwrap().is_infinite());
        assert!(python_float("-nan").unwrap().is_nan());
        assert_eq!(python_float("--1"), None);
        assert_eq!(python_float("1e"), None);
        assert_eq!(python_float("infinit"), None);
        assert_eq!(cast_number("12"), Some(Number::Int(12)));
        assert_eq!(cast_number("12.0"), Some(Number::Float(12.0)));
    }

    #[test]
    fn float_repr_matches_python() {
        let cases = [
            (0.1, "0.1"),
            (1.0, "1.0"),
            (-0.0, "-0.0"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (123456789012345680.0, "1.2345678901234568e+17"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1.5e-7, "1.5e-07"),
            (5e-324, "5e-324"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
            (2.5, "2.5"),
            (100.0, "100.0"),
            (f64::NAN, "nan"),
            (f64::NEG_INFINITY, "-inf"),
        ];
        for (x, want) in cases {
            assert_eq!(float_repr(x), want, "{x:e}");
        }
        assert_eq!(number_text(&Number::Int(-5)), "-5");
        assert_eq!(number_text(&Number::BigInt("99".into())), "99");
    }

    #[test]
    fn excel_datetimes_match_openpyxl() {
        let f = |v: f64| excel_datetime_text(&Number::Float(v), false);
        assert_eq!(f(0.5).as_deref(), Some("12:00:00"));
        assert_eq!(
            excel_datetime_text(&Number::Int(0), false).as_deref(),
            Some("00:00:00")
        );
        assert_eq!(f(1.0).as_deref(), Some("1900-01-01 00:00:00"));
        assert_eq!(f(0.99999999999).as_deref(), Some("1900-01-01 00:00:00"));
        assert_eq!(f(59.0).as_deref(), Some("1900-02-28 00:00:00"));
        assert_eq!(f(60.0).as_deref(), Some("1900-02-28 00:00:00"));
        assert_eq!(f(61.0).as_deref(), Some("1900-03-01 00:00:00"));
        assert_eq!(f(36526.75).as_deref(), Some("2000-01-01 18:00:00"));
        assert_eq!(f(-1.5).as_deref(), Some("1899-12-28 12:00:00"));
        assert_eq!(f(1e10), None);
        assert_eq!(f(f64::NAN), None);
        assert_eq!(f(0.0000001).as_deref(), Some("00:00:00.009000"));
        assert_eq!(
            excel_datetime_text(&Number::Int(1), true).as_deref(),
            Some("1904-01-02 00:00:00")
        );
        assert_eq!(
            excel_datetime_text(&Number::BigInt("1".repeat(50)), false),
            None
        );
    }

    #[test]
    fn excel_timedeltas_match_openpyxl() {
        let f = |v: f64| excel_timedelta_text(&Number::Float(v));
        assert_eq!(f(1.5).as_deref(), Some("1 day, 12:00:00"));
        assert_eq!(f(0.25).as_deref(), Some("6:00:00"));
        assert_eq!(f(2.0).as_deref(), Some("2 days, 0:00:00"));
        assert_eq!(f(-0.25).as_deref(), Some("-1 day, 18:00:00"));
        assert_eq!(
            excel_timedelta_text(&Number::Int(3)).as_deref(),
            Some("3 days, 0:00:00")
        );
        assert_eq!(f(1e12), None);
        assert_eq!(f(1.0000000001).as_deref(), Some("1 day, 0:00:00"));
        assert_eq!(f(0.123456789).as_deref(), Some("2:57:46.667000"));
        assert_eq!(f(f64::INFINITY), None);
    }

    #[test]
    fn iso8601_matches_openpyxl() {
        assert_eq!(iso8601_text("2001-05-04").as_deref(), Some("2001-05-04"));
        assert_eq!(
            iso8601_text("2001-05-04T10:20:30Z").as_deref(),
            Some("2001-05-04 10:20:30")
        );
        assert_eq!(iso8601_text("10:20").as_deref(), Some("10:20:00"));
        assert_eq!(iso8601_text("2001-13-04"), None);
        assert_eq!(
            iso8601_text("2001-05-04T10:20:30.123").as_deref(),
            Some("2001-05-04 10:20:30.123000")
        );
        assert_eq!(iso8601_text("nonsense"), None);
    }
}
