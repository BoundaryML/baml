// TODO: This file has been simplified to remove baml_runtime/baml_log dependencies.

// TODO: baml_runtime is disabled for now
// use baml_runtime::RuntimeCliDefaults;

use std::{ffi::OsStr, io::Write as _, path::Path};

fn main() {
    // TODO: baml_log is disabled for now
    // baml_log::init()?;

    warn_if_direct_invocation();

    let argv: Vec<String> = std::env::args().collect();

    // Route every top-level error through the ariadne-style printer
    // (bold-red "Error:" header + cause chain) so plain bails look the
    // same as compiler diagnostics. Returning `Result<()>` from `main`
    // would defer to anyhow's Debug printer instead, which renders the
    // header uncolored.
    let exit_code = match baml_cli::run_cli(argv) {
        Ok(code) => code,
        Err(e) => {
            baml_cli::reporter::print_anyhow_error(&e);
            baml_cli::ExitCode::Other
        }
    };
    std::process::exit(exit_code.into());
}

fn warn_if_direct_invocation() {
    if env_flag("BAML_WRAPPER_EXEC") || env_flag("BAML_CLI_ALLOW_DIRECT") {
        return;
    }
    if is_canonical_baml_invocation(std::env::args_os().next().as_deref()) {
        return;
    }
    let _ = writeln!(
        std::io::stderr(),
        "warning: using the internal BAML toolchain binary directly is not recommended. Use `baml` instead."
    );
}

fn is_canonical_baml_invocation(invocation: Option<&OsStr>) -> bool {
    invocation.is_some_and(|invocation| {
        let file_name = Path::new(invocation).file_name().unwrap_or(invocation);
        canonical_baml_executable_name(file_name)
    })
}

fn canonical_baml_executable_name(file_name: &OsStr) -> bool {
    #[cfg(windows)]
    {
        Path::new(file_name)
            .file_stem()
            .is_some_and(|stem| stem == OsStr::new("baml"))
    }

    #[cfg(not(windows))]
    {
        file_name == OsStr::new("baml")
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::is_canonical_baml_invocation;

    #[test]
    fn canonical_baml_invocation_is_not_direct() {
        assert!(is_canonical_baml_invocation(Some(OsStr::new("baml"))));
        assert!(is_canonical_baml_invocation(Some(OsStr::new(
            "/home/user/.baml/toolchains/nightly/bin/baml",
        ))));
    }

    #[test]
    fn internal_cli_invocation_is_direct() {
        assert!(!is_canonical_baml_invocation(Some(OsStr::new("baml-cli",))));
        assert!(!is_canonical_baml_invocation(Some(OsStr::new(
            "/home/user/.baml/toolchains/nightly/bin/baml-cli",
        ))));
        assert!(!is_canonical_baml_invocation(None));
    }

    #[test]
    fn executable_extension_only_counts_as_baml_on_windows() {
        assert_eq!(
            is_canonical_baml_invocation(Some(OsStr::new("baml.exe"))),
            cfg!(windows),
        );
    }
}
