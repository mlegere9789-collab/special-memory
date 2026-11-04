//! Integer-only TOML reader shared by every rules loader.
//!
//! [`parse_table`] parses with the `toml` crate through serde into a private value tree that has
//! no floating-point payload: a float token is recorded as a marker and refused with
//! [`DataError::Float`] before any loader sees the data. A loader then takes the keys it knows
//! from a [`Table`] and calls [`Table::finish`], which refuses whatever is left
//! ([`DataError::UnknownKey`]).

use crate::DataError;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::collections::BTreeMap;
use std::fmt;

/// A parsed TOML value. Floats keep no number, only the fact that one was written.
#[derive(Debug)]
enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
    Float,
    Array(Vec<Value>),
    Table(BTreeMap<String, Value>),
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Value, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Value;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a TOML value")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
                Ok(Value::Bool(v))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
                Ok(Value::Int(v))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
                i64::try_from(v)
                    .map(Value::Int)
                    .map_err(|_| E::custom("integer beyond i64"))
            }
            // The only place a float can arrive; its value is discarded unread.
            fn visit_f64<E: de::Error>(self, _discarded: f64) -> Result<Value, E> {
                Ok(Value::Float)
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
                Ok(Value::Str(v.to_owned()))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
                let mut items = Vec::new();
                while let Some(v) = a.next_element()? {
                    items.push(v);
                }
                Ok(Value::Array(items))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
                let mut map = BTreeMap::new();
                while let Some((k, v)) = a.next_entry::<String, Value>()? {
                    map.insert(k, v);
                }
                Ok(Value::Table(map))
            }
        }
        d.deserialize_any(V)
    }
}

/// A TOML table whose keys are consumed by the loader; leftovers are unknown keys.
#[derive(Debug)]
pub struct Table {
    path: String,
    entries: BTreeMap<String, Value>,
}

/// Parses `text` as a TOML document and returns its root table.
///
/// Errors: [`DataError::Parse`] for malformed TOML (including duplicate keys and integers beyond
/// `i64`), [`DataError::Float`] for the first float token found (keys in ascending order).
pub fn parse_table(text: &str) -> Result<Table, DataError> {
    let root: BTreeMap<String, Value> =
        toml::from_str(text).map_err(|e| DataError::Parse(e.message().to_owned()))?;
    let table = Table {
        path: String::new(),
        entries: root,
    };
    reject_floats(&table.entries, &table.path)?;
    Ok(table)
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

fn reject_floats(map: &BTreeMap<String, Value>, path: &str) -> Result<(), DataError> {
    for (k, v) in map {
        reject_value(v, &join(path, k))?;
    }
    Ok(())
}

fn reject_value(v: &Value, path: &str) -> Result<(), DataError> {
    match v {
        Value::Float => Err(DataError::Float(path.to_owned())),
        Value::Table(m) => reject_floats(m, path),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .try_for_each(|(i, x)| reject_value(x, &format!("{path}[{i}]"))),
        Value::Int(_) | Value::Bool(_) | Value::Str(_) => Ok(()),
    }
}

impl Table {
    /// Dotted path of this table in the document (empty for the root).
    pub fn path(&self) -> &str {
        &self.path
    }

    fn key_path(&self, key: &str) -> String {
        join(&self.path, key)
    }

    fn missing(&self, key: &str) -> DataError {
        DataError::Parse(format!("missing key {}", self.key_path(key)))
    }

    fn wrong_type(&self, key: &str, want: &str) -> DataError {
        DataError::Parse(format!("{} must be {want}", self.key_path(key)))
    }

    /// Removes and returns an integer; absent key or other type is [`DataError::Parse`].
    pub fn take_int(&mut self, key: &str) -> Result<i64, DataError> {
        self.take_int_opt(key)?.ok_or_else(|| self.missing(key))
    }

    /// Like [`Table::take_int`], but an absent key gives `None`.
    pub fn take_int_opt(&mut self, key: &str) -> Result<Option<i64>, DataError> {
        match self.entries.remove(key) {
            None => Ok(None),
            Some(Value::Int(v)) => Ok(Some(v)),
            Some(_) => Err(self.wrong_type(key, "an integer")),
        }
    }

    /// Removes an integer that must lie in `lo..=hi`, else [`DataError::Range`].
    pub fn take_int_in(&mut self, key: &str, lo: i64, hi: i64) -> Result<i64, DataError> {
        let v = self.take_int(key)?;
        if v < lo || v > hi {
            return Err(DataError::Range(format!(
                "{} = {v} outside {lo}..={hi}",
                self.key_path(key)
            )));
        }
        Ok(v)
    }

    /// Removes and returns a string.
    pub fn take_str(&mut self, key: &str) -> Result<String, DataError> {
        self.take_str_opt(key)?.ok_or_else(|| self.missing(key))
    }

    /// Like [`Table::take_str`], but an absent key gives `None`.
    pub fn take_str_opt(&mut self, key: &str) -> Result<Option<String>, DataError> {
        match self.entries.remove(key) {
            None => Ok(None),
            Some(Value::Str(s)) => Ok(Some(s)),
            Some(_) => Err(self.wrong_type(key, "a string")),
        }
    }

    /// Removes and returns a boolean.
    pub fn take_bool(&mut self, key: &str) -> Result<bool, DataError> {
        self.take_bool_opt(key)?.ok_or_else(|| self.missing(key))
    }

    /// Like [`Table::take_bool`], but an absent key gives `None`.
    pub fn take_bool_opt(&mut self, key: &str) -> Result<Option<bool>, DataError> {
        match self.entries.remove(key) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(b)),
            Some(_) => Err(self.wrong_type(key, "a boolean")),
        }
    }

    /// Removes and returns a sub-table (`[name]`).
    pub fn take_table(&mut self, key: &str) -> Result<Table, DataError> {
        match self.entries.remove(key) {
            None => Err(self.missing(key)),
            Some(Value::Table(entries)) => Ok(Table {
                path: self.key_path(key),
                entries,
            }),
            Some(_) => Err(self.wrong_type(key, "a table")),
        }
    }

    /// Removes an array of tables (`[[name]]`, or inline tables), in file order.
    pub fn take_rows(&mut self, key: &str) -> Result<Vec<Table>, DataError> {
        self.take_rows_opt(key)?.ok_or_else(|| self.missing(key))
    }

    /// Like [`Table::take_rows`], but an absent key gives `None`.
    pub fn take_rows_opt(&mut self, key: &str) -> Result<Option<Vec<Table>>, DataError> {
        let base = self.key_path(key);
        match self.entries.remove(key) {
            None => Ok(None),
            Some(Value::Array(items)) => items
                .into_iter()
                .enumerate()
                .map(|(i, v)| match v {
                    Value::Table(entries) => Ok(Table {
                        path: format!("{base}[{i}]"),
                        entries,
                    }),
                    _ => Err(DataError::Parse(format!("{base}[{i}] must be a table"))),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Some),
            Some(_) => Err(DataError::Parse(format!(
                "{base} must be an array of tables"
            ))),
        }
    }

    /// Ends the read: any key still present is [`DataError::UnknownKey`] (smallest key first).
    pub fn finish(self) -> Result<(), DataError> {
        match self.entries.keys().next() {
            None => Ok(()),
            Some(k) => Err(DataError::UnknownKey(join(&self.path, k))),
        }
    }
}
