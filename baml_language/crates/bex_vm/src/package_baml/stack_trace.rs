use bex_vm_types::Value;

use super::{BamlClassErrorsStackTrace, BamlNamespaceErrors, PackageBamlImpl, view};
use crate::{BexVm, errors::format_traceback};

impl BamlNamespaceErrors for PackageBamlImpl {}

impl BamlClassErrorsStackTrace for PackageBamlImpl {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn to_string(vm: &BexVm, stacktrace: &Value) -> String {
        let instance = vm
            .as_instance(stacktrace)
            .expect("StackTrace: expected Instance");
        let st = view::errors::StackTrace { instance };
        let frames = st.frames(vm);

        let frame_views: Vec<_> = frames
            .iter()
            .map(|frame_value| {
                let frame_instance = vm
                    .as_instance(frame_value)
                    .expect("StackFrame: expected Instance");
                view::errors::StackFrame {
                    instance: frame_instance,
                }
            })
            .collect();

        let out = format_traceback(frame_views.iter().map(|frame| {
            (
                frame.file(vm),
                frame.line() as usize,
                frame.function_name(vm),
            )
        }));

        out.trim_end().to_string()
    }
}
