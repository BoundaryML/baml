//! `trace.Options` builders: immutable tracing settings owned by the loop
//! thread, built and inspected exactly as the interpreter's `package_trace`.
use crate::{Dyn, FLOAT_TAG, R, strings::Str};
use btel_types::{
    InvocationMode,
    context::{ContextPatch, ContextValue},
};

use crate::native::OPAQUE_DESC;

#[derive(Clone, Default)]
pub struct Options {
    pub mode: Option<InvocationMode>,
    pub inputs: Option<bool>,
    pub output: Option<bool>,
    pub error: Option<bool>,
    pub context: Option<ContextPatch>,
}

fn store(options: Options) -> R<usize> {
    Ok(crate::opaque::Opaque::new(OPAQUE_DESC, options)?.as_raw())
}

/// # Safety
/// `state` is a live `trace.Options` handle.
unsafe fn read(state: usize) -> Options {
    unsafe { crate::opaque::Opaque::<Options>::from_raw(state) }.with(Clone::clone)
}

/// The variant index of `trace.Mode` (`Hidden`, `Timing`, `Span`).
fn mode(index: Option<u64>) -> Option<InvocationMode> {
    index.map(|i| match i {
        0 => InvocationMode::Hidden,
        1 => InvocationMode::Timing,
        _ => InvocationMode::Span,
    })
}

/// A nullable bool argument; omitted reads as null.
fn flag(value: Dyn) -> Option<bool> {
    (value.tag == 1).then_some(value.payload != 0)
}

/// `trace.options(mode)`, `hidden()`, `timing()`: `mode` is a `trace.Mode`
/// variant index or `None`.
pub fn with_mode(mode_index: Option<u64>) -> R<usize> {
    store(Options {
        mode: mode(mode_index),
        ..Options::default()
    })
}

/// `trace._span(inputs, output, error)` over an optional base.
/// # Safety
/// A non-zero `base` is a live `trace.Options` handle.
pub unsafe fn span(base: usize, inputs: Dyn, output: Dyn, error: Dyn) -> R<usize> {
    let mut options = if base == 0 {
        Options::default()
    } else {
        unsafe { read(base) }
    };
    if base == 0 {
        options.context = None;
    }
    options.mode = Some(InvocationMode::Span);
    options.inputs = flag(inputs);
    options.output = flag(output);
    options.error = flag(error);
    store(options)
}

/// `Options.mode(mode)`.
/// # Safety
/// `base` is a live `trace.Options` handle.
pub unsafe fn set_mode(base: usize, mode_index: Option<u64>) -> R<usize> {
    let mut options = unsafe { read(base) };
    options.mode = mode(mode_index);
    store(options)
}

/// A context patch from a `map<string, string | int | float | bool | null>`
/// argument's entries and an optional distinct id.
/// # Safety
/// String payloads are live.
pub unsafe fn patch(entries: &[(String, Dyn)], distinct_id: usize) -> ContextPatch {
    ContextPatch {
        metadata: entries
            .iter()
            .map(|(key, value)| {
                let value = match value.tag {
                    0 => Some(ContextValue::Int(value.payload as i64)),
                    1 => Some(ContextValue::Bool(value.payload != 0)),
                    FLOAT_TAG => Some(ContextValue::Float(f64::from_bits(value.payload))),
                    3 if value.payload != 0 => Some(ContextValue::String(
                        unsafe { Str::from_raw(value.payload as usize) }.to_owned(),
                    )),
                    _ => None,
                };
                (key.clone(), value)
            })
            .collect(),
        distinct_id: (distinct_id != 0).then(|| unsafe { Str::from_raw(distinct_id) }.to_owned()),
    }
}

/// `trace.context(...)` / `Options.context(...)`: compose `patch` over the
/// base's context; an empty composition is no context.
/// # Safety
/// A non-zero `base` is a live `trace.Options` handle.
pub unsafe fn context(base: usize, patch: ContextPatch) -> R<usize> {
    let mut options = if base == 0 {
        Options::default()
    } else {
        unsafe { read(base) }
    };
    let mut combined = options.context.take().unwrap_or_default();
    combined.compose(patch);
    options.context = (!combined.is_empty()).then_some(combined);
    store(options)
}

/// The detached settings an `inspect()` reports.
pub struct Settings {
    /// `trace.Mode` variant index.
    pub mode: Option<u64>,
    pub inputs: Option<bool>,
    pub output: Option<bool>,
    pub error: Option<bool>,
    pub distinct_id: Option<String>,
    pub metadata: Vec<(String, Option<ContextValue>)>,
}

/// # Safety
/// `base` is a live `trace.Options` handle.
pub unsafe fn inspect(base: usize) -> Settings {
    let options = unsafe { read(base) };
    let patch = options.context.unwrap_or_default();
    Settings {
        mode: options.mode.map(|m| match m {
            InvocationMode::Hidden => 0,
            InvocationMode::Timing => 1,
            InvocationMode::Span => 2,
        }),
        inputs: options.inputs,
        output: options.output,
        error: options.error,
        distinct_id: patch.distinct_id,
        metadata: patch.metadata.into_iter().collect(),
    }
}

/// A context metadata value as a managed `string | int | float | bool | null`.
pub fn metadata_value(value: &Option<ContextValue>) -> R<Dyn> {
    Ok(match value {
        None => Dyn { tag: 2, payload: 0 },
        Some(ContextValue::Int(v)) => Dyn {
            tag: 0,
            payload: *v as u64,
        },
        Some(ContextValue::Bool(v)) => Dyn {
            tag: 1,
            payload: u64::from(*v),
        },
        Some(ContextValue::Float(v)) => Dyn {
            tag: FLOAT_TAG,
            payload: v.to_bits(),
        },
        Some(ContextValue::String(v)) => Dyn {
            tag: 3,
            payload: unsafe { crate::strings::Cx::current() }.string(v)?.as_raw() as u64,
        },
    })
}

/// A nullable bool field value.
pub fn optional_bool(value: Option<bool>) -> Dyn {
    value.map_or(Dyn { tag: 2, payload: 0 }, |v| Dyn {
        tag: 1,
        payload: u64::from(v),
    })
}
