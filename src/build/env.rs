//! The template environment: bundled templates and static files, Jinja2-style
//! HTML escaping, and the few filters, functions and Python methods the
//! templates call.

use minijinja::value::ValueKind;
use minijinja::{AutoEscape, Environment, Error, ErrorKind, Output, State, Value};

const TEMPLATES: [(&str, &str); 13] = [
    ("base.html", include_str!("../../templates/base.html")),
    ("_macros.html", include_str!("../../templates/_macros.html")),
    (
        "_preseason_table.html",
        include_str!("../../templates/_preseason_table.html"),
    ),
    ("week.html", include_str!("../../templates/week.html")),
    ("week0.html", include_str!("../../templates/week0.html")),
    ("landing.html", include_str!("../../templates/landing.html")),
    ("season.html", include_str!("../../templates/season.html")),
    (
        "season_recap.html",
        include_str!("../../templates/season_recap.html"),
    ),
    ("draft.html", include_str!("../../templates/draft.html")),
    (
        "rivalries.html",
        include_str!("../../templates/rivalries.html"),
    ),
    ("rosters.html", include_str!("../../templates/rosters.html")),
    ("trades.html", include_str!("../../templates/trades.html")),
    (
        "how_it_works.html",
        include_str!("../../templates/how_it_works.html"),
    ),
];

pub const STATIC_FILES: [(&str, &[u8]); 2] = [
    ("app.js", include_bytes!("../../static/app.js")),
    ("style.css", include_bytes!("../../static/style.css")),
];

pub fn environment() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.set_auto_escape_callback(|name: &str| {
        if name.ends_with(".html") {
            AutoEscape::Html
        } else {
            AutoEscape::None
        }
    });
    env.set_formatter(formatter);
    env.set_unknown_method_callback(python_method);
    for (name, source) in TEMPLATES {
        env.add_template(name, source)
            .expect("bundled templates parse");
    }
    env.add_filter("fmt2", fmt2);
    env.add_filter("pct", pct);
    env.add_filter("thousands", thousands);
    env.add_filter("xmlattr", xmlattr);
    env.add_function("week_url", week_url);
    env.add_function("year_url", year_url);
    env
}

/// The Python methods the templates call: `dict.get(key, default=None)` and
/// `list.index(item)`.
fn python_method(_: &State, value: &Value, method: &str, args: &[Value]) -> Result<Value, Error> {
    match (value.kind(), method, args) {
        (ValueKind::Map, "get", [key, rest @ ..]) if rest.len() <= 1 => {
            let found = value.get_item(key)?;
            if found.is_undefined() {
                Ok(rest.first().cloned().unwrap_or(Value::from(())))
            } else {
                Ok(found)
            }
        }
        (ValueKind::Seq, "index", [item]) => value
            .try_iter()?
            .position(|candidate| &candidate == item)
            .map(Value::from)
            .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "item is not in list")),
        _ => Err(Error::from(ErrorKind::UnknownMethod)),
    }
}

/// Jinja2's `xmlattr`: ` key="value"` for each entry, skipping none values,
/// with the values escaped.
fn xmlattr(value: Value) -> Result<Value, Error> {
    let mut out = String::new();
    for key in value.try_iter()? {
        let item = value.get_item(&key)?;
        if item.is_none() || item.is_undefined() {
            continue;
        }
        out.push_str(&format!(
            " {}=\"{}\"",
            escape(&key.to_string()),
            escape(&item.to_string())
        ));
    }
    Ok(Value::from_safe_string(out))
}

/// HTML escaping with exactly Jinja2's (markupsafe's) character set, so the
/// output matches the Python build byte for byte.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&#34;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn formatter(out: &mut Output, state: &State, value: &Value) -> Result<(), Error> {
    if state.auto_escape() == AutoEscape::Html && !value.is_safe() {
        out.write_str(&escape(&value.to_string()))
            .map_err(|_| Error::new(minijinja::ErrorKind::WriteFailure, "write failed"))
    } else {
        minijinja::escape_formatter(out, state, value)
    }
}

fn number(value: &Value) -> f64 {
    f64::try_from(value.clone()).unwrap_or(0.0)
}

/// `"%.2f"`.
fn fmt2(value: Value) -> String {
    format!("{:.2}", number(&value))
}

/// A simulated probability. The extremes are hedged: 0 of 10,000 runs is
/// unlikely, not impossible, since clinching isn't worked out exactly.
fn pct(value: Value) -> String {
    let share = number(&value);
    if share <= 0.0 {
        "<0.1%".to_string()
    } else if share >= 1.0 {
        ">99.9%".to_string()
    } else {
        format!("{:.1}%", share * 100.0)
    }
}

/// Python's `"{:,}".format(n)`.
fn thousands(value: Value) -> String {
    let n = number(&value) as i64;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

fn week_url(week: i64) -> String {
    format!("weeks/week-{week}.html")
}

fn year_url(year: String) -> String {
    format!("{year}/index.html")
}
