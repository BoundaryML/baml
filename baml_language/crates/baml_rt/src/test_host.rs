//! Process-isolated host for packed test bodies. No interpreter is linked.
use crate::{Instance, R};
use baml_rt_gc::Descriptor;

pub fn run(
    descriptors: &'static [Descriptor],
    entry: fn() -> R<i64>,
    functions: &'static [crate::telemetry::Function],
    sites: &'static [crate::telemetry::Site],
    name: &str,
) -> std::process::ExitCode {
    let mut session = match crate::telemetry::Session::new(functions, sites) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e.message);
            return std::process::ExitCode::FAILURE;
        }
    };
    let mut pointers = vec![false; 3];
    pointers.extend(vec![true; descriptors.len()]);
    let mut instance = Instance::new(descriptors, Box::leak(pointers.into_boxed_slice()));
    // SAFETY: same admitted entry ABI as driver_observed; the instance owns the heap.
    let result = session.run(|| unsafe { instance.run_baml(entry) });
    session.finish(&result);
    let (outcome, message) = match result {
        Ok(_) => ("pass", None),
        Err(e) if e.value == 0 => {
            eprintln!("VM internal error: {}", e.message);
            return std::process::ExitCode::FAILURE;
        }
        Err(e) => ("fail", Some(failure_message(&e))),
    };
    println!(
        "BAML_CORPUS_RESULT\t{}",
        serde_json::json!({"name":name,"outcome":outcome,"message":message})
    );
    if outcome == "pass" {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::from(2)
    }
}

fn failure_message(error: &crate::Error) -> String {
    if matches!(
        error.class,
        "baml.panics.UserPanic" | "baml.panics.AssertionFailed"
    ) {
        return error.message.clone();
    }
    // Match testing._run_test's errors.Context.to_string, whose stack renderer
    // trims the traceback before adding a newline and the structural value.
    let traceback = bex_vm_types::errors::format_traceback(
        error
            .frames
            .iter()
            .rev()
            .map(|f| (f.file, f.line, f.function)),
    );
    let value = error
        .display_value
        .clone()
        .unwrap_or_else(|| format!("{} {{ message: {:?} }}", error.class, error.message));
    format!("{}\n{value}", traceback.trim_end())
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn error(class: &'static str) -> crate::Error {
        crate::Error {
            telemetry_raise: None,
            telemetry_at: None,
            telemetry_caught: None,
            value: 1,
            class,
            message: "boom".into(),
            frames: vec![crate::Frame {
                file: "main.baml",
                function: "user.test",
                line: 2,
            }],
            display_value: Some(format!("{class} {{ code: 1 }}")),
        }
    }
    #[test]
    fn test_failure_uses_context_rendering_not_cli_uncaught_error_rendering() {
        assert_eq!(
            failure_message(&error("user.Error")),
            "Traceback (most recent call last):\n  File \"main.baml\", line 2, in user.test\nuser.Error { code: 1 }"
        );
        assert_eq!(
            failure_message(&error("baml.panics.AssertionFailed")),
            "boom"
        );
        assert_eq!(failure_message(&error("baml.panics.UserPanic")), "boom");
        assert!(failure_message(&error("user.UserPanic")).contains("Traceback"));
    }
}
