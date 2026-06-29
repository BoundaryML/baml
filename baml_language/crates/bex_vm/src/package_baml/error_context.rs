//! Native impl for `baml.errors.ErrorContext` (BEP-042 Part 3 cause chain).
//!
//! `to_string` renders the full cause chain Python-style: oldest error first,
//! then "During handling of the above error, another error occurred:" before
//! each newer link. The chain is stored newest → oldest via `cause`, so we
//! collect it head-first and render it reversed.

use std::fmt::Write;

use bex_vm_types::types::Value;

use super::{
    BamlClassErrorsErrorContext, BamlClassErrorsStackTrace, PackageBamlImpl,
    unstable::format_value_recursive, view,
};
use crate::BexVm;

const CHAIN_SEPARATOR: &str = "\n\nDuring handling of the above error, another error occurred:\n\n";

impl BamlClassErrorsErrorContext for PackageBamlImpl {
    // Calls the generated `StackTrace::_to_string_impl` to render each link's
    // trace; the underscore prefix is the native-builtin convention.
    #[allow(clippy::used_underscore_items)]
    fn _to_string_impl(vm: &BexVm, ctx: &view::errors::ErrorContext<'_>) -> bex_str::BexStr {
        // Collect the chain head-first (newest → oldest).
        let mut links: Vec<(Value, Value)> = vec![(ctx.error(), ctx.stack_trace())];
        let mut cause = ctx.cause(vm);
        while let Some(cause_value) = cause {
            let instance = vm
                .as_instance(&cause_value)
                .expect("ErrorContext.cause: expected Instance");
            let link = view::errors::ErrorContext { instance };
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
                .expect("ErrorContext.stack_trace: expected Instance");
            let st_view = view::errors::StackTrace {
                instance: st_instance,
            };
            let trace =
                <PackageBamlImpl as BamlClassErrorsStackTrace>::_to_string_impl(vm, &st_view);
            let _ = write!(out, "{trace}");

            let rendered =
                format_value_recursive(vm, *error, 0).unwrap_or_else(|_| "<error>".to_string());
            let _ = write!(out, "\n{rendered}");
        }

        bex_str::BexStr::from(out.trim_end().to_string())
    }
}
