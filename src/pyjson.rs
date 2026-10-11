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
        // serde_json prints the shortest round-trip form, as Python's repr does.
        serde_json::to_string(&f).unwrap_or_default()
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
