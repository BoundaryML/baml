//! Native impl for `baml.errors.baml.errors.Context` (BEP-042 Part 3 cause chain).
//!
//! `to_string` renders the full cause chain Python-style: oldest error first,
//! then "During handling of the above error, another error occurred:" before
//! each newer link. The chain is stored newest → oldest via `cause`, so we
//! collect it head-first and render it reversed.

use std::fmt::Write;

use bex_vm_types::types::Value;

use super::{BamlClassErrorsContext, BamlClassErrorsStackTrace, PackageBamlImpl, view};
use crate::BexVm;

/// Render a thrown error value for the chain trace. Error instances retain
/// their qualified class name and fields so a caught-and-reported test failure
/// is as actionable as an error escaping directly from `baml run`.
fn render_error_value(vm: &BexVm, value: Value) -> String {
    if let Ok(message) = vm.as_string(&value) {
        return message.to_string();
    }
    super::root::render_value_structural(vm, value, false)
}

const CHAIN_SEPARATOR: &str = "\n\nDuring handling of the above error, another error occurred:\n\n";

impl BamlClassErrorsContext for PackageBamlImpl {
    // Calls the generated `StackTrace::_to_string_impl` to render each link's
    // trace; the underscore prefix is the native-builtin convention.
    #[allow(clippy::used_underscore_items)]
    fn _to_string_impl(vm: &BexVm, ctx: &view::errors::Context<'_>) -> bex_str::BexStr {
        // Collect the chain head-first (newest → oldest).
        let mut links: Vec<(Value, Value)> = vec![(ctx.error(), ctx.stack_trace())];
        let mut cause = ctx.cause(vm);
        while let Some(cause_value) = cause {
            let instance = vm
                .as_instance(&cause_value)
                .expect("baml.errors.Context.cause: expected Instance");
            let link = view::errors::Context { instance };
            links.push((link.error(), link.stack_trace()));
            cause = link.cause(vm);
        }

        // Render oldest → newest, Python-style.
        let mut out = String::new();
        for (i, (error, stack_trace)) in links.iter().rev().enumerate() {
            if i > 0 {
                out.push_str(CHAIN_SEPARATOR);
            }

            let st_instance = vm
                .as_instance(stack_trace)
                .expect("baml.errors.Context.stack_trace: expected Instance");
            let st_view = view::errors::StackTrace {
                instance: st_instance,
            };
            let trace =
                <PackageBamlImpl as BamlClassErrorsStackTrace>::_to_string_impl(vm, &st_view);
            let _ = write!(out, "{trace}");

            let _ = write!(out, "\n{}", render_error_value(vm, *error));
        }

        bex_str::BexStr::from(out.trim_end().to_string())
    }
}
