//! `baml.regex` — pattern compilation and matching (`$rust_function`).
//!
//! The engines, the dialect split, and the error classification all live in
//! `sys_regex`, which the compiler also uses to check a *constant* pattern at
//! build time. This module is only the marshalling layer: `Value` in, `Value`
//! out.
//!
//! # Errors are a construction-time concern
//!
//! `compile` and `word` throw `baml.regex.Error`; nothing else here throws.
//! That is the point of a compiled `Regex` value: a program holding one is past
//! the only step that could reject the pattern. A backtracking search can still
//! run out of budget, which panics rather than reporting a false "no match" —
//! see [`aborted`].
//!
//! # Offsets
//!
//! `sys_regex` reports byte offsets; BAML strings are indexed by codepoint.
//! Every span crossing the boundary goes through `sys_regex::char_offsets`,
//! which converts a whole batch in one pass over the haystack rather than
//! re-counting from the start for each span.

use std::{any::Any, sync::Arc};

use bex_heap::TlabHolder;
use bex_str::BexStr;
use bex_vm_types::{
    RealizedTy,
    types::{Object, Value},
};
use indexmap::IndexMap;
use sys_regex::{BuildError, Program, RawMatch, SearchAborted};

use super::{BamlClassRegexRegex, BamlNamespaceRegex, PackageBamlImpl, copy};
use crate::{
    BexVm,
    errors::{VmInternalError, VmPanic, VmRustFnError},
};

/// Fully-qualified name of the enum classifying a `baml.regex.Error`.
const ERROR_KIND_FQN: &str = "baml.regex.ErrorKind";

// =============================================================================
// Building BAML values
// =============================================================================

/// A codepoint offset as a BAML `int`.
///
/// Bounded by the haystack's byte length, which the heap already caps well
/// below `i64::MAX`, so the cast cannot lose information.
#[expect(
    clippy::cast_possible_wrap,
    reason = "a codepoint offset is bounded by the haystack's byte length"
)]
const fn offset_int(chars: usize) -> i64 {
    chars as i64
}

fn offset_value(chars: usize) -> Value {
    Value::try_int(offset_int(chars)).unwrap_or(Value::NULL)
}

fn group_value(vm: &mut BexVm, hay: &BexStr, span: (usize, usize), chars: (usize, usize)) -> Value {
    let text = Value::object(vm.alloc_string(hay.substring(span.0, span.1)));
    copy::regex::Group {
        text,
        start: offset_int(chars.0),
        end: offset_int(chars.1),
    }
    .to_value(vm)
}

/// Materialize one match as a `baml.regex.Match`.
///
/// Every span in the match — the whole match and each participating group — is
/// converted to codepoint offsets in a single pass over `hay`, so a match with
/// many groups costs one traversal rather than one per group.
fn match_value(vm: &mut BexVm, hay: &BexStr, raw: &RawMatch, names: &[Option<String>]) -> Value {
    let spans: Vec<usize> = raw
        .groups
        .iter()
        .flatten()
        .flat_map(|&(start, end)| [start, end])
        .collect();
    let chars = sys_regex::char_offsets(hay.as_str(), &spans);

    // `groups` is built first so each per-group `Value` can be reused for the
    // `named` map: a named group is the same `Group`, reachable two ways.
    let mut cursor = 0usize;
    let mut groups: Vec<Value> = Vec::with_capacity(raw.groups.len());
    for span in &raw.groups {
        match *span {
            Some(span) => {
                let offsets = (chars[cursor], chars[cursor + 1]);
                cursor += 2;
                groups.push(group_value(vm, hay, span, offsets));
            }
            None => groups.push(Value::NULL),
        }
    }

    // Every name in the pattern gets a key, participating or not: code reading
    // `named` should not have to distinguish "no such group" from "did not
    // match this time".
    let mut named: IndexMap<BexStr, Value> = IndexMap::new();
    for (index, name) in names.iter().enumerate() {
        if let Some(name) = name {
            let group = groups.get(index).copied().unwrap_or(Value::NULL);
            named.insert(BexStr::from(name.as_str()), group);
        }
    }

    // Group 0 is the whole match and always participates, so its text and
    // offsets are the match's own.
    let (start_byte, end_byte) = raw.span();
    let text = Value::object(vm.alloc_string(hay.substring(start_byte, end_byte)));
    let (start, end) = if raw.groups.first().is_some_and(Option::is_some) {
        (chars[0], chars[1])
    } else {
        (0, 0)
    };

    let groups_value = Value::object(vm.alloc_array(RealizedTy::unknown(), groups));
    let named_value =
        Value::object(vm.alloc_map(RealizedTy::string(), RealizedTy::unknown(), named));

    copy::regex::Match {
        text,
        start: offset_int(start),
        end: offset_int(end),
        groups: groups_value,
        named: named_value,
    }
    .to_value(vm)
}

fn error_value(vm: &mut BexVm, pattern: &str, err: &BuildError) -> Result<Value, VmRustFnError> {
    let enum_ptr = vm.lookup_type_by_fqn(ERROR_KIND_FQN).ok_or_else(|| {
        VmInternalError::MissingNativeFunction {
            name: ERROR_KIND_FQN.to_string(),
        }
    })?;
    let variant = match vm.get_object(enum_ptr) {
        Object::Enum(en) => en
            .variants
            .iter()
            .position(|v| v.name == err.kind.variant_name()),
        _ => None,
    }
    .ok_or_else(|| VmInternalError::MissingNativeFunction {
        name: format!("{ERROR_KIND_FQN}.{}", err.kind.variant_name()),
    })?;

    // The engine reports byte offsets into the pattern; the BAML class
    // documents them as codepoint offsets, the same units `Match` uses.
    let (span_start, span_end) = match err.span {
        Some((start, end)) => {
            let chars = sys_regex::char_offsets(pattern, &[start, end]);
            (offset_value(chars[0]), offset_value(chars[1]))
        }
        None => (Value::NULL, Value::NULL),
    };

    let kind = Value::object(vm.alloc_variant(enum_ptr, variant));
    let message = Value::object(vm.alloc_string(err.message.clone()));
    let pattern = Value::object(vm.alloc_string(pattern.to_owned()));
    Ok(copy::regex::Error {
        kind,
        message,
        pattern,
        span_start,
        span_end,
    }
    .to_value(vm))
}

fn throw_build_error(vm: &mut BexVm, pattern: &str, err: &BuildError) -> VmRustFnError {
    match error_value(vm, pattern, err) {
        Ok(value) => VmRustFnError::thrown_fresh(value),
        Err(fatal) => fatal,
    }
}

fn regex_value(vm: &mut BexVm, prog: Program) -> Value {
    let handle: Arc<dyn Any + Send + Sync> = Arc::new(prog);
    copy::regex::Regex { _handle: handle }.to_value(vm)
}

/// Clone the `Arc` out of `Regex._handle` so the program stays alive while
/// `&mut BexVm` is borrowed for allocation.
fn program_of(vm: &BexVm, regex: Value) -> Result<Arc<Program>, VmRustFnError> {
    let instance = vm.as_instance(&regex)?;
    let ptr = instance.load_field(0).as_object_ptr().ok_or_else(|| {
        VmInternalError::MissingNativeFunction {
            name: "baml.regex.Regex._handle is not an object".to_string(),
        }
    })?;
    match vm.get_object(ptr) {
        Object::RustData(data) => data.clone().downcast::<Program>().map_err(|_| {
            VmRustFnError::InternalError(VmInternalError::MissingNativeFunction {
                name: "baml.regex.Regex._handle holds an unexpected Rust type".to_string(),
            })
        }),
        _ => Err(VmRustFnError::InternalError(
            VmInternalError::MissingNativeFunction {
                name: "baml.regex.Regex._handle is not RustData".to_string(),
            },
        )),
    }
}

/// A search that ran out of budget is a panic, not a "no match".
///
/// Reachable only from a `backtracking = true` pattern: the default engine has
/// no budget to exhaust. Reporting it as "no match" would turn a resource limit
/// into a wrong answer, silently.
fn aborted(prog: &Program, abort: SearchAborted) -> VmRustFnError {
    VmPanic::UserPanic {
        message: format!(
            "baml.regex: {} while matching /{}/ — this pattern was compiled with \
             `backtracking = true`, whose matching time is not bounded",
            abort.reason,
            prog.pattern()
        ),
    }
    .into()
}

// =============================================================================
// Trait implementations
// =============================================================================

impl BamlClassRegexRegex for PackageBamlImpl {
    fn is_match(vm: &mut BexVm, regex: &Value, haystack: &BexStr) -> Result<bool, VmRustFnError> {
        let prog = program_of(vm, *regex)?;
        prog.is_match(haystack.as_str())
            .map_err(|abort| aborted(&prog, abort))
    }

    fn match_(
        vm: &mut BexVm,
        regex: &Value,
        haystack: &BexStr,
    ) -> Result<Option<Value>, VmRustFnError> {
        let prog = program_of(vm, *regex)?;
        let found = prog
            .find_first(haystack.as_str())
            .map_err(|abort| aborted(&prog, abort))?;
        Ok(found.map(|raw| match_value(vm, haystack, &raw, prog.names())))
    }

    fn match_all(
        vm: &mut BexVm,
        regex: &Value,
        haystack: &BexStr,
    ) -> Result<Vec<Value>, VmRustFnError> {
        let prog = program_of(vm, *regex)?;
        let found = prog
            .find_all(haystack.as_str())
            .map_err(|abort| aborted(&prog, abort))?;
        Ok(found
            .iter()
            .map(|raw| match_value(vm, haystack, raw, prog.names()))
            .collect())
    }

    fn exact_match(
        vm: &mut BexVm,
        regex: &Value,
        haystack: &BexStr,
    ) -> Result<Option<Value>, VmRustFnError> {
        let prog = program_of(vm, *regex)?;
        let found = prog
            .find_exact(haystack.as_str())
            .map_err(|abort| aborted(&prog, abort))?;
        Ok(found.map(|raw| match_value(vm, haystack, &raw, prog.names())))
    }

    fn split(
        vm: &mut BexVm,
        regex: &Value,
        haystack: &BexStr,
    ) -> Result<Vec<BexStr>, VmRustFnError> {
        let prog = program_of(vm, *regex)?;
        let pieces = prog
            .split(haystack.as_str())
            .map_err(|abort| aborted(&prog, abort))?;
        // Every piece — segment or interleaved delimiter capture — is a
        // substring of the haystack, so each is a zero-copy slice.
        Ok(pieces
            .into_iter()
            .map(|(start, end)| haystack.substring(start, end))
            .collect())
    }

    fn _replace_template(
        vm: &mut BexVm,
        regex: &Value,
        haystack: &BexStr,
        template: &BexStr,
    ) -> Result<BexStr, VmRustFnError> {
        replace_template(vm, *regex, haystack, template, 1)
    }

    fn _replace_all_template(
        vm: &mut BexVm,
        regex: &Value,
        haystack: &BexStr,
        template: &BexStr,
    ) -> Result<BexStr, VmRustFnError> {
        replace_template(vm, *regex, haystack, template, 0)
    }
}

/// Template replacement, shared by the first-match (`limit = 1`) and
/// every-match (`limit = 0`) entry points.
fn replace_template(
    vm: &mut BexVm,
    regex: Value,
    haystack: &BexStr,
    template: &BexStr,
    limit: usize,
) -> Result<BexStr, VmRustFnError> {
    let prog = program_of(vm, regex)?;
    let replaced = prog
        .replacen(haystack.as_str(), limit, template.as_str())
        .map_err(|abort| aborted(&prog, abort))?;
    Ok(match replaced {
        // Nothing matched: hand back the original, which keeps the result a
        // slice of the same allocation rather than a fresh copy.
        std::borrow::Cow::Borrowed(_) => haystack.clone(),
        std::borrow::Cow::Owned(s) => BexStr::from(s),
    })
}

impl BamlNamespaceRegex for PackageBamlImpl {
    fn _compile(
        vm: &mut BexVm,
        pattern: &BexStr,
        backtracking: bool,
    ) -> Result<Value, VmRustFnError> {
        match Program::compile(pattern.as_str(), backtracking) {
            Ok(prog) => Ok(regex_value(vm, prog)),
            Err(err) => Err(throw_build_error(vm, pattern.as_str(), &err)),
        }
    }

    fn _word(vm: &mut BexVm, literal: &BexStr, ignore_case: bool) -> Result<Value, VmRustFnError> {
        match Program::word(literal.as_str(), ignore_case) {
            Ok(prog) => Ok(regex_value(vm, prog)),
            // The literal is escaped, so the only way here is an input the
            // engine still rejects (a size limit). The error's span refers to
            // the pattern `word` built, so report that pattern, not the literal.
            Err(err) => Err(throw_build_error(
                vm,
                &Program::word_pattern(literal.as_str(), ignore_case),
                &err,
            )),
        }
    }

    fn escape(literal: &BexStr) -> BexStr {
        BexStr::from(sys_regex::escape(literal.as_str()))
    }
}
