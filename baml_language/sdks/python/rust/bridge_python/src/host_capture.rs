//! Copy exact builtin values only. Capture never invokes a Python serializer,
//! iterator protocol, property, repr or exception display hook.
use std::collections::HashSet;

use btel_snapshot::{
    Limit,
    host::{HostValue, MAX_BYTES, MAX_DEPTH, MAX_VALUES},
};
use pyo3::{
    prelude::*,
    types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple},
};

pub fn capture(value: &Bound<'_, PyAny>) -> HostValue {
    let mut remaining = MAX_VALUES;
    let mut bytes = MAX_BYTES;
    copy(value, 0, &mut remaining, &mut bytes, &mut HashSet::new())
}

fn copy(
    value: &Bound<'_, PyAny>,
    depth: usize,
    remaining: &mut usize,
    bytes: &mut usize,
    active: &mut HashSet<usize>,
) -> HostValue {
    if depth > MAX_DEPTH {
        return HostValue::Truncated(Limit::Depth);
    }
    if *remaining == 0 {
        return HostValue::Truncated(Limit::Values);
    }
    *remaining -= 1;
    if value.is_none() {
        return HostValue::Null;
    }
    if value.is_exact_instance_of::<PyBool>() {
        return value
            .extract::<bool>()
            .map_or(HostValue::Unavailable, HostValue::Bool);
    }
    if value.is_exact_instance_of::<PyInt>() {
        return value
            .extract::<i64>()
            .map_or(HostValue::Unavailable, HostValue::Int);
    }
    if value.is_exact_instance_of::<PyFloat>() {
        return value
            .extract::<f64>()
            .map_or(HostValue::Unavailable, HostValue::Float);
    }
    if value.is_exact_instance_of::<PyString>() {
        // Unicode's character count needs no UTF-8 cache allocation. Bound it
        // before conversion; at most four bytes per retained character can
        // then be materialized, and the actual byte count is checked below.
        if value.len().is_ok_and(|length| length > *bytes) {
            return HostValue::Truncated(Limit::Bytes);
        }
        let Ok(text) = value.cast::<PyString>().expect("exact string").to_str() else {
            return HostValue::Unavailable;
        };
        if text.len() > *bytes {
            return HostValue::Truncated(Limit::Bytes);
        }
        *bytes -= text.len();
        return HostValue::String(text.to_owned());
    }
    let identity = value.as_ptr() as usize;
    if !active.insert(identity) {
        return HostValue::Unavailable;
    }
    let result = if value.is_exact_instance_of::<PyList>() {
        let list = value.cast::<PyList>().expect("exact list");
        let mut values = Vec::new();
        for item in list.iter() {
            if *remaining == 0 {
                values.push(HostValue::Truncated(Limit::Values));
                break;
            }
            values.push(copy(&item, depth + 1, remaining, bytes, active));
        }
        HostValue::List(values)
    } else if value.is_exact_instance_of::<PyTuple>() {
        let tuple = value.cast::<PyTuple>().expect("exact tuple");
        let mut values = Vec::new();
        for item in tuple.iter() {
            if *remaining == 0 {
                values.push(HostValue::Truncated(Limit::Values));
                break;
            }
            values.push(copy(&item, depth + 1, remaining, bytes, active));
        }
        HostValue::List(values)
    } else if value.is_exact_instance_of::<PyDict>() {
        let dictionary = value.cast::<PyDict>().expect("exact dict");
        let mut entries = Vec::new();
        for (key, item) in dictionary.iter() {
            if *remaining == 0 {
                active.remove(&identity);
                return HostValue::Truncated(Limit::Values);
            }
            if !key.is_exact_instance_of::<PyString>() {
                active.remove(&identity);
                return HostValue::Unavailable;
            }
            if key.len().is_ok_and(|length| length > *bytes) {
                active.remove(&identity);
                return HostValue::Truncated(Limit::Bytes);
            }
            let Ok(key) = key.cast::<PyString>().expect("exact string").to_str() else {
                active.remove(&identity);
                return HostValue::Unavailable;
            };
            if key.len() > *bytes {
                active.remove(&identity);
                return HostValue::Truncated(Limit::Bytes);
            }
            *bytes -= key.len();
            entries.push((
                key.to_owned(),
                copy(&item, depth + 1, remaining, bytes, active),
            ));
        }
        HostValue::Map(entries)
    } else {
        HostValue::Unavailable
    };
    active.remove(&identity);
    result
}
