//! Reading JSON text: the scanner every text path shares, and `parse`, which
//! builds `json` values from the text in one pass.
//!
//! The scanner accepts exactly what `serde_json::from_str` accepts (RFC 8259,
//! with `serde_json`'s nesting limit), and on top of that rejects an integral
//! number past the bigint limit, as `parse` always has, and a number with a
//! fraction or exponent past `f64` (`1e400`), which JSON cannot mean as a
//! finite number. A scan that fails reports only that it failed: the error
//! message is recovered by [`syntax_error`], which re-reads the text with
//! `serde_json` so the message (and its line and column) stays the one
//! `parse` has always given.

use std::sync::Arc;

use bex_heap::TlabHolder;
use bex_vm_types::{
    MapData, Object, RealizedTy, Value,
    types::{Array, Map},
};
use rustc_hash::FxHashMap;

use crate::{BexVm, errors::VmRustFnError};

pub(super) use bex_vm_types::json_text::{
    Number, Reader, Refusal, Stop, bigint_limit_message, refusal,
};

// ─── parse ───────────────────────────────────────────────────────────────────

/// Build the `json` value `text` holds, or [`Stop`] when it is not valid JSON
/// (see [`syntax_error`] for the message).
pub(super) fn parse(vm: &mut BexVm, text: &str) -> Result<Value, Stop> {
    let mut parser = Parser {
        reader: Reader::new(text),
        json: super::json_alias_ty(vm),
    };
    let value = parser.value(vm)?;
    parser.reader.finish()?;
    Ok(value)
}

/// Builds `json` values. Nothing it allocates can move until the native
/// returns: the collector only runs while every VM is yielded.
pub(super) struct Parser<'a> {
    pub(super) reader: Reader<'a>,
    /// The `json` alias, the element and value type of every container built.
    pub(super) json: RealizedTy,
}

impl Parser<'_> {
    /// The value at the cursor.
    pub(super) fn value(&mut self, vm: &mut BexVm) -> Result<Value, Stop> {
        let reader = &mut self.reader;
        match reader.peek().ok_or(Stop)? {
            b'"' => {
                let text = reader.string()?;
                Ok(Value::object(vm.alloc_string(text)))
            }
            b'[' => {
                reader.open()?;
                let mut items = Vec::new();
                while self.reader.next_element(items.is_empty())? {
                    items.push(self.value(vm)?);
                }
                Ok(Value::object(vm.alloc(Object::Array(Array::new(
                    self.json.clone(),
                    items,
                )))))
            }
            b'{' => {
                reader.open()?;
                let mut entries = Entries::default();
                while let Some(key) = self.reader.next_key(entries.is_empty())? {
                    let key = bex_vm_types::BexStr::from(key);
                    let value = self.value(vm)?;
                    entries.insert(key, value);
                }
                Ok(entries.alloc(vm, RealizedTy::String, self.json.clone()))
            }
            b't' | b'f' => reader.bool().map(Value::bool),
            b'n' => reader.null().map(|()| Value::NULL),
            _ => {
                let number = reader.number()?;
                number_value(vm, number)
            }
        }
    }
}

/// A number token as an untyped `json` value: an `int` when it is integral
/// and fits BAML's 63-bit `int`, a `bigint` when it is integral and larger, a
/// `float` otherwise. A float past `f64`, and an integral token past the
/// bigint limit, are [`Stop`].
pub(super) fn number_value(vm: &mut BexVm, number: Number<'_>) -> Result<Value, Stop> {
    if !number.integral {
        let float = number.as_f64().ok_or(Stop)?;
        return Ok(Value::object(vm.alloc_float(float)));
    }
    if let Some(int) = number.as_i64().and_then(Value::try_int) {
        return Ok(int);
    }
    let bigint = number.as_bigint().ok_or(Stop)?;
    Ok(Value::object(vm.alloc(Object::Bigint(Arc::new(bigint)))))
}

/// The entries of a JSON object being built, keyed by text: a repeated key
/// keeps its first position and takes its last value, as it always has.
#[derive(Default)]
pub(super) struct Entries {
    entries: Vec<(u64, bex_vm_types::BexStr, Value)>,
    /// Positions by key hash, once there are enough entries that a scan for
    /// a repeated key costs more than the index.
    index: Option<FxHashMap<u64, Vec<usize>>>,
}

/// Entries an object holds before repeated keys are found through an index.
const ENTRIES_SCANNED: usize = 32;

impl Entries {
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(super) fn insert(&mut self, key: bex_vm_types::BexStr, value: Value) {
        let hash = bex_vm_types::map_string_hash(&key);
        let entries = &mut self.entries;
        let seen = match &self.index {
            None => entries
                .iter()
                .position(|(seen, seen_key, _)| *seen == hash && *seen_key == key),
            Some(index) => index
                .get(&hash)
                .and_then(|positions| positions.iter().copied().find(|&i| entries[i].1 == key)),
        };
        if let Some(position) = seen {
            entries[position].2 = value;
            return;
        }
        entries.push((hash, key, value));
        if let Some(index) = &mut self.index {
            index.entry(hash).or_default().push(entries.len() - 1);
        } else if entries.len() > ENTRIES_SCANNED {
            let mut index: FxHashMap<u64, Vec<usize>> = FxHashMap::default();
            for (position, (hash, _, _)) in entries.iter().enumerate() {
                index.entry(*hash).or_default().push(position);
            }
            self.index = Some(index);
        }
    }

    /// The map of these entries, typed `map<key, value>`.
    pub(super) fn alloc(self, vm: &mut BexVm, key: RealizedTy, value: RealizedTy) -> Value {
        let mut data = MapData::new();
        for (hash, key, value) in self.entries {
            let key = Value::object(vm.alloc_string(key));
            data.insert_hashed(hash, key, value);
        }
        Value::object(vm.alloc(Object::Map(Map::new(key, value, data))))
    }
}

// ─── Errors ──────────────────────────────────────────────────────────────────

/// The `ParseError` for `text`, which a scan refused. The message is
/// `serde_json`'s (`expected value at line 1 column 3`), or names the number
/// past a limit when that is all that is wrong.
pub(super) fn syntax_error(vm: &mut BexVm, text: &str) -> VmRustFnError {
    let message = match refusal(text) {
        Refusal::Syntax(message) | Refusal::OutOfRange(message) => message,
        Refusal::BigintLimit => bigint_limit_message(),
    };
    VmRustFnError::thrown_fresh(super::throw_json_parse_error(vm, message))
}
