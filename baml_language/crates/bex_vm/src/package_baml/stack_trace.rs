use bex_vm_types::Value;

use super::{BamlClassErrorsStackTrace, BamlNamespaceErrors, PackageBamlImpl, view};
use crate::BexVm;

impl BamlNamespaceErrors for PackageBamlImpl {}

impl BamlClassErrorsStackTrace for PackageBamlImpl {
    fn to_string(vm: &BexVm, stacktrace: &Value) -> String {
        use std::fmt::Write;

        let instance = vm
            .as_instance(stacktrace)
            .expect("StackTrace: expected Instance");
        let st = view::errors::StackTrace { instance };
        let frames = st.frames(vm);

        let mut out = String::new();
        if !frames.is_empty() {
            writeln!(out, "Traceback (most recent call last):").unwrap();
            for frame_value in frames {
                let frame_instance = vm
                    .as_instance(frame_value)
                    .expect("StackFrame: expected Instance");
                let frame = view::errors::StackFrame {
                    instance: frame_instance,
                };
                writeln!(
                    out,
                    "  File \"{}\", line {}, in {}",
                    frame.file(vm),
                    frame.line(),
                    frame.function_name(vm),
                )
                .unwrap();
            }
        }

        out.trim_end().to_string()
    }
}
