//! A reader for the small integer-only TOML subset that `index.toml` uses (ADR 0004).
//!
//! Supported: comments, `key = value` with a basic string (escapes `\"` and `\\` only), a decimal
//! integer, or a one-line array of strings; `[table]` and `[[array of tables]]` headers. Floats,
//! booleans, dates, inline tables and multi-line values are refused, as are duplicate keys.

use crate::TerrainError;
use std::collections::BTreeMap;

/// A parsed value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Val {
    Int(i64),
    Str(String),
    Strs(Vec<String>),
}

/// The keys of one table.
pub(crate) type Table = BTreeMap<String, Val>;

/// A parsed document.
#[derive(Default)]
pub(crate) struct Doc {
    /// Keys before the first header.
    pub root: Table,
    /// `[name]` tables.
    pub tables: BTreeMap<String, Table>,
    /// `[[name]]` rows in file order.
    pub arrays: Vec<(String, Table)>,
}

fn err(line: usize, msg: &str) -> TerrainError {
    TerrainError::Index(format!("line {line}: {msg}"))
}

fn parse_string(s: &str, line: usize) -> Result<(String, &str), TerrainError> {
    let mut out = String::new();
    let mut chars = s.char_indices();
    chars.next(); // the opening quote
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &s[i + 1..])),
            '\\' => match chars.next() {
                Some((_, e @ ('"' | '\\'))) => out.push(e),
                _ => return Err(err(line, "unsupported escape")),
            },
            c => out.push(c),
        }
    }
    Err(err(line, "unterminated string"))
}

fn parse_value(s: &str, line: usize) -> Result<Val, TerrainError> {
    let s = s.trim();
    if s.starts_with('"') {
        let (v, rest) = parse_string(s, line)?;
        return if rest.trim().is_empty() {
            Ok(Val::Str(v))
        } else {
            Err(err(line, "text after string"))
        };
    }
    if let Some(mut rest) = s.strip_prefix('[') {
        let mut items = Vec::new();
        loop {
            rest = rest.trim_start();
            if let Some(r) = rest.strip_prefix(']') {
                return if r.trim().is_empty() {
                    Ok(Val::Strs(items))
                } else {
                    Err(err(line, "text after array"))
                };
            }
            if !rest.starts_with('"') {
                return Err(err(line, "arrays hold strings"));
            }
            let (v, r) = parse_string(rest, line)?;
            items.push(v);
            rest = r.trim_start();
            if let Some(r) = rest.strip_prefix(',') {
                rest = r;
            } else if !rest.starts_with(']') {
                return Err(err(line, "bad array"));
            }
        }
    }
    let digits = s.strip_prefix('-').unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(err(line, "value is not an integer, string or string array"));
    }
    s.parse::<i64>()
        .map(Val::Int)
        .map_err(|_| err(line, "integer out of range"))
}

/// Removes a trailing comment (a `#` outside a string).
fn strip_comment(l: &str) -> &str {
    let (mut in_str, mut esc) = (false, false);
    for (i, c) in l.char_indices() {
        match c {
            _ if esc => esc = false,
            '\\' if in_str => esc = true,
            '"' => in_str = !in_str,
            '#' if !in_str => return &l[..i],
            _ => {}
        }
    }
    l
}

fn valid_key(k: &str) -> bool {
    !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Parses `text` into a [`Doc`].
pub(crate) fn parse(text: &str) -> Result<Doc, TerrainError> {
    enum Cur {
        Root,
        Table(String),
        Row,
    }
    let mut doc = Doc::default();
    let mut cur = Cur::Root;
    for (n, raw) in text.lines().enumerate() {
        let line = n + 1;
        let l = strip_comment(raw).trim();
        if l.is_empty() {
            continue;
        }
        if let Some(h) = l.strip_prefix("[[") {
            let name = h.strip_suffix("]]").map(str::trim).filter(|n| valid_key(n));
            let name = name.ok_or_else(|| err(line, "bad table header"))?;
            doc.arrays.push((name.to_string(), Table::new()));
            cur = Cur::Row;
        } else if let Some(h) = l.strip_prefix('[') {
            let name = h.strip_suffix(']').map(str::trim).filter(|n| valid_key(n));
            let name = name.ok_or_else(|| err(line, "bad table header"))?;
            if doc.tables.insert(name.to_string(), Table::new()).is_some() {
                return Err(err(line, "duplicate table"));
            }
            cur = Cur::Table(name.to_string());
        } else {
            let (k, v) = l
                .split_once('=')
                .ok_or_else(|| err(line, "expected key = value"))?;
            let k = k.trim();
            if !valid_key(k) {
                return Err(err(line, "bad key"));
            }
            let v = parse_value(v, line)?;
            let table = match &cur {
                Cur::Root => &mut doc.root,
                Cur::Table(name) => doc
                    .tables
                    .get_mut(name)
                    .ok_or_else(|| err(line, "no table"))?,
                Cur::Row => doc
                    .arrays
                    .last_mut()
                    .map(|r| &mut r.1)
                    .ok_or_else(|| err(line, "no row"))?,
            };
            if table.insert(k.to_string(), v).is_some() {
                return Err(err(line, "duplicate key"));
            }
        }
    }
    Ok(doc)
}
