//! `json.dumps` as Python writes it: `ensure_ascii` escaping, `", "` and
//! `": "` separators, and an optional indent. Output must match Python's
//! byte for byte, since the asset-version hash covers `data.js`.

use serde_json::Value;

pub fn dumps(value: &Value, indent: Option<usize>) -> String {
    let mut out = String::new();
    write_value(value, indent, 0, &mut out);
    out
}

fn write_value(value: &Value, indent: Option<usize>, depth: usize, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&number(n)),
        Value::String(s) => write_string(s, out),
        Value::Array(items) => {
            write_container(items.len(), indent, depth, out, '[', ']', |i, out| {
                write_value(&items[i], indent, depth + 1, out)
            })
        }
        Value::Object(map) => {
            let entries: Vec<(&String, &Value)> = map.iter().collect();
            write_container(entries.len(), indent, depth, out, '{', '}', |i, out| {
                write_string(entries[i].0, out);
                out.push_str(": ");
                write_value(entries[i].1, indent, depth + 1, out);
            })
        }
    }
}

fn write_container(
    len: usize,
    indent: Option<usize>,
    depth: usize,
    out: &mut String,
    open: char,
    close: char,
    mut item: impl FnMut(usize, &mut String),
) {
    out.push(open);
    if len == 0 {
        out.push(close);
        return;
    }
    for i in 0..len {
        if i > 0 {
            out.push(',');
        }
        match indent {
            Some(width) => {
                out.push('\n');
                out.push_str(&" ".repeat(width * (depth + 1)));
            }
            None if i > 0 => out.push(' '),
            None => {}
        }
        item(i, out);
    }
    if let Some(width) = indent {
        out.push('\n');
        out.push_str(&" ".repeat(width * depth));
    }
    out.push(close);
}

fn number(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    let f = n.as_f64().unwrap_or(f64::NAN);
    if f.is_nan() {
        "NaN".to_string()
    } else if f.is_infinite() {
        if f > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
    } else {
        float_repr(f)
    }
}

/// Python's `repr(float)`: the shortest digits that round-trip, written as a
/// plain decimal when the exponent is in [-4, 16) and as `d.ddde±XX` (at least
/// two exponent digits) otherwise.
fn float_repr(f: f64) -> String {
    if f == 0.0 {
        return if f.is_sign_negative() { "-0.0" } else { "0.0" }.to_string();
    }
    // `{:e}` gives the same shortest round-trip digits Python uses.
    let scientific = format!("{f:e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("exponent form");
    let exponent: i32 = exponent.parse().expect("integer exponent");
    let sign = if mantissa.starts_with('-') { "-" } else { "" };
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    if (-4..16).contains(&exponent) {
        let point = exponent + 1; // digits before the decimal point
        if point <= 0 {
            format!(
                "{sign}0.{}{digits}",
                "0".repeat(point.unsigned_abs() as usize)
            )
        } else if point as usize >= digits.len() {
            format!(
                "{sign}{digits}{}.0",
                "0".repeat(point as usize - digits.len())
            )
        } else {
            let (whole, fraction) = digits.split_at(point as usize);
            format!("{sign}{whole}.{fraction}")
        }
    } else {
        let (first, rest) = digits.split_at(1);
        let mantissa = if rest.is_empty() {
            first.to_string()
        } else {
            format!("{first}.{rest}")
        };
        let exp_sign = if exponent < 0 { '-' } else { '+' };
        format!("{sign}{mantissa}e{exp_sign}{:02}", exponent.unsigned_abs())
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ' '..='~' => out.push(ch),
            _ => {
                let mut units = [0u16; 2];
                for unit in ch.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{:04x}", unit));
                }
            }
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_print_exactly_as_python_repr() {
        let cases = [
            (5e-6, "5e-06"),
            (-3.5e-7, "-3.5e-07"),
            (0.00009999, "9.999e-05"),
            (2.5e-5, "2.5e-05"),
            (1e16, "1e+16"),
            (1.5e16, "1.5e+16"),
            (1.4210854715202004e-14, "1.4210854715202004e-14"),
            (0.0001, "0.0001"),
            (0.0003, "0.0003"),
            (0.03125, "0.03125"),
            (1e-14, "1e-14"),
            (28.0, "28.0"),
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1234567890123456.0, "1234567890123456.0"),
            (1e22, "1e+22"),
            (123.456, "123.456"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1e-5, "1e-05"),
        ];
        for (value, python) in cases {
            assert_eq!(float_repr(value), python, "repr({value:e})");
        }
    }

    #[test]
    fn dumps_matches_python_separators_and_escapes() {
        let value = serde_json::json!({"a": [1, 2.5, "\u{e9}"], "b": {}});
        assert_eq!(
            dumps(&value, None),
            "{\"a\": [1, 2.5, \"\\u00e9\"], \"b\": {}}"
        );
        assert_eq!(
            dumps(&value, Some(2)),
            "{\n  \"a\": [\n    1,\n    2.5,\n    \"\\u00e9\"\n  ],\n  \"b\": {}\n}"
        );
    }
}
