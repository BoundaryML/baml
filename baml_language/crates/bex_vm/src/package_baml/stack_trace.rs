use super::{BamlClassErrorsStackTrace, BamlNamespaceErrors, PackageBamlImpl, view};
use crate::{BexVm, errors::format_traceback};

impl BamlNamespaceErrors for PackageBamlImpl {}

impl BamlClassErrorsStackTrace for PackageBamlImpl {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn _to_string_impl(vm: &BexVm, stacktrace: &view::errors::StackTrace<'_>) -> String {
        let frames = stacktrace.frames(vm);

        let frame_views: Vec<_> = frames
            .iter()
            .map(|frame_value| {
                let frame_instance = vm
                    .as_instance(frame_value)
                    .expect("StackFrame: expected Instance");
                view::errors::StackFrame::new(frame_instance)
            })
            .collect();

        let frames: Vec<_> = frame_views
            .iter()
            .map(|frame| (frame.file(vm), frame.line() as usize, frame.function_name(vm)))
            .collect();
        let out = format_traceback(
            frames
                .iter()
                .map(|(file, line, function)| (&**file, *line, &**function)),
        );

        out.trim_end().to_string()
    }
}
