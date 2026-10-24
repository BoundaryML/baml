//! The selector walk behind `baml.json.path<T>`: the selector is read once,
//! and each step moves through the `json` value it names.
//!
//! Positions count characters, not bytes, as BAML strings do, so an offset in
//! a message points at the same character a BAML `slice` would.

use bex_heap::TlabHolder;
use bex_vm_types::{Object, Value};

use crate::{BexVm, errors::VmRustFnError};

/// The node `selector` selects in `json`, or the `PathError` saying which
/// step failed. `selector` is the error's `selector`, whole.
pub(super) fn select(vm: &mut BexVm, json: Value, selector: &str) -> Result<Value, VmRustFnError> {
    if !(selector.starts_with('.') || selector.starts_with('[')) {
        return Err(refuse(
            vm,
            format!("selector must start with '.' or '[' (jq style), got '{selector}'"),
            selector,
        ));
    }
    let chars: Vec<char> = selector.chars().collect();
    let mut current = json;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '.' {
            // A dot step runs to the next `.` or `[`; a bare `.` selects
            // nothing.
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '.' && chars[i] != '[' {
                i += 1;
            }
            if i > start {
                let key: String = chars[start..i].iter().collect();
                current = field(vm, current, &key, selector)?;
            }
            continue;
        }
        if chars[i] != '[' {
            return Err(refuse(
                vm,
                format!("expected '.' or '[' at offset {i}"),
                selector,
            ));
        }
        i += 1;
        let Some(&first) = chars.get(i) else {
            return Err(refuse(vm, "unterminated '[' segment", selector));
        };
        if first == '"' || first == '\'' {
            // A quoted key; `\` takes the next character as written.
            i += 1;
            let mut key = String::new();
            let mut closed = false;
            while let Some(&ch) = chars.get(i) {
                if ch == '\\' {
                    let Some(&escaped) = chars.get(i + 1) else {
                        return Err(refuse(
                            vm,
                            "unterminated escape in quoted bracket key",
                            selector,
                        ));
                    };
                    key.push(escaped);
                    i += 2;
                    continue;
                }
                i += 1;
                if ch == first {
                    closed = true;
                    break;
                }
                key.push(ch);
            }
            if !closed || chars.get(i) != Some(&']') {
                return Err(refuse(vm, "unterminated quoted bracket key", selector));
            }
            i += 1;
            current = field(vm, current, &key, selector)?;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i] != ']' {
            i += 1;
        }
        if i >= chars.len() {
            return Err(refuse(vm, "unterminated '[' segment", selector));
        }
        let token: String = chars[start..i].iter().collect();
        i += 1;
        // An index is what `int.parse` reads: an optional sign and ASCII
        // digits, within `int`'s range.
        let Some(index) = token
            .parse::<i64>()
            .ok()
            .filter(|index| (Value::INT_MIN..=Value::INT_MAX).contains(index))
        else {
            return Err(refuse(
                vm,
                format!("bad bracket token '[{token}]'"),
                selector,
            ));
        };
        current = element(vm, current, index, selector)?;
    }
    Ok(current)
}

/// The value under `key` in the object `current`.
fn field(
    vm: &mut BexVm,
    current: Value,
    key: &str,
    selector: &str,
) -> Result<Value, VmRustFnError> {
    let object = current.as_object_ptr().filter(
        |&ptr| matches!(vm.get_object(ptr), Object::Map(map) if super::string_keyed(vm, map)),
    );
    let Some(ptr) = object else {
        return Err(refuse(
            vm,
            format!("field access '.{key}' on a non-object"),
            selector,
        ));
    };
    match super::object_get(vm, ptr, bex_vm_types::map_string_hash(key), key) {
        Some(value) => Ok(value),
        None => Err(refuse(vm, format!("missing field '{key}'"), selector)),
    }
}

/// The element at `index` of the array `current`. Indexes are absolute:
/// a negative one is out of bounds.
fn element(
    vm: &mut BexVm,
    current: Value,
    index: i64,
    selector: &str,
) -> Result<Value, VmRustFnError> {
    let length = match current.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
        Some(Object::Array(array)) => {
            if let Some(value) = usize::try_from(index).ok().and_then(|i| array.get(i)) {
                return Ok(value);
            }
            array.len()
        }
        _ => {
            return Err(refuse(
                vm,
                format!("index [{index}] on a non-array"),
                selector,
            ));
        }
    };
    Err(refuse(
        vm,
        format!("index [{index}] out of bounds (length {length})"),
        selector,
    ))
}

/// A `PathError` with `message`, for the whole `selector`.
fn refuse(vm: &mut BexVm, message: impl Into<String>, selector: &str) -> VmRustFnError {
    let class = vm.lang_object(crate::lang_items::BAML_JSON_PATH_ERROR);
    let message = Value::object(vm.alloc_string(message.into()));
    let selector = Value::object(vm.alloc_string(selector));
    VmRustFnError::thrown_fresh(Value::object(
        vm.alloc_instance(class, vec![message, selector]),
    ))
}
