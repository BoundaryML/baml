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
    session.finish(result.is_ok());
    let (outcome, message) = match result {
        Ok(_) => ("pass", None),
        Err(e) if e.value == 0 => {
            eprintln!("VM internal error: {}", e.message);
            return std::process::ExitCode::FAILURE;
        }
        Err(e) => {
            let message = if e.class.ends_with("UserPanic") || e.class.ends_with("AssertionFailed")
            {
                e.message.clone()
            } else {
                let traceback = bex_vm_types::errors::format_traceback(
                    e.frames.iter().rev().map(|f| (f.file, f.line, f.function)),
                );
                format!(
                    "{traceback}uncaught throw: {}",
                    e.display_value
                        .clone()
                        .unwrap_or_else(|| format!("{} {{message: {:?}}}", e.class, e.message))
                )
            };
            ("fail", Some(message))
        }
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
