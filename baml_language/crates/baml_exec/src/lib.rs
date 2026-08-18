// Shared execution helpers for `baml run` and `baml pack`.
//
// The run verb (in baml_cli) and the packaged-binary host (baml_pack_host)
// share a target-dispatch contract: given a `BexEngine` holding a compiled
// program, a target function name, and a token stream derived from the
// user's command line, parse those tokens against the target's typed
// signature (auto-CLI from BEP-027), invoke the function, and serialize
// the return value to stdout in the configured `OutputFormat`.
//
// This crate owns that contract so run and pack behave identically at the
// dispatch boundary. Target resolution (scripts, namespace shorthand,
// hermetic file loading) stays in the caller — pack deliberately doesn't
// support `[scripts]`, so keeping resolution out of here avoids paying
// for it in the host binary.

pub mod auto_cli;
mod call_context;
pub mod clap_target;
pub mod diag_print;
pub mod dispatch;
pub mod envelope;
pub mod json_coerce;
mod log_output;
pub mod output;

pub use auto_cli::{is_auto_cli_primitive, parse_cli_value};
pub use call_context::CallContextCapture;
pub use clap_target::{CLAP_STYLING, ParsedTargetArgs, parse_multi_target_argv, parse_target_argv};
pub use diag_print::{print_anyhow_error, print_error, print_warning};

/// Subset of `clap` re-exported so downstream binaries (pack-host) can
/// classify [`parse_target_argv`] errors without taking a direct clap
/// dep — keeps the host's dependency footprint trimmed to `baml_exec`.
pub mod clap_reexport {
    pub use clap::{Error, error::ErrorKind};
}
pub use dispatch::{
    DispatchResult, build_args_from_signature, clamp_exit_code, dispatch_target,
    dispatch_target_with_context, validate_help_param,
};
pub use envelope::{PACK_SECTION_NAME, PackEnvelope, PackMode, TargetEntry};
pub use json_coerce::load_json_source;
pub use log_output::{LogLevel, LogOutput};
pub use output::{OutputFormat, format_value, write_output, write_output_with_context};
