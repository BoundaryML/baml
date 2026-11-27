//! Links a compiled object with the runtime archive into an executable,
//! using the system C compiler as the linker driver.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

/// The runtime archives this build of the compiler was built with, each
/// with the system libraries it needs as linker arguments: without
/// `baml.http` and with it, for [`RuntimeBuild::Lean`] and then for
/// [`RuntimeBuild::Checked`].
///
/// A TLS library sets itself up when a program that links it starts, and
/// brings system frameworks to load, whether or not a request is ever sent,
/// and the linker cannot drop either. So only a program that calls a native
/// of `baml.http` is linked against a runtime that has it.
const RUNTIMES: [[(&str, &str); 2]; 2] = [
    [
        (
            env!("BAML_NATIVE_RT_ARCHIVE"),
            env!("BAML_NATIVE_RT_LINK_LIBS"),
        ),
        (
            env!("BAML_NATIVE_RT_HTTP_ARCHIVE"),
            env!("BAML_NATIVE_RT_HTTP_LINK_LIBS"),
        ),
    ],
    [
        (
            env!("BAML_NATIVE_RT_CHECKED_ARCHIVE"),
            env!("BAML_NATIVE_RT_CHECKED_LINK_LIBS"),
        ),
        (
            env!("BAML_NATIVE_RT_HTTP_CHECKED_ARCHIVE"),
            env!("BAML_NATIVE_RT_HTTP_CHECKED_LINK_LIBS"),
        ),
    ],
];

/// Which build of the runtime a program is linked against.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RuntimeBuild {
    /// What a program ships with.
    #[default]
    Lean,
    /// What a test links: it can run under migration stress
    /// (`BAML_MIGRATE_STRESS`), and it ends the program when a parking
    /// task's stack holds a frame that is not compiled code or when a
    /// compiled frame has no stack map. The second thread of migration
    /// stress costs every start about 0.6 M instructions.
    Checked,
}

/// How the symbol of every native of `baml.http` starts.
const HTTP_NATIVE: &[u8] = b"baml_rt_baml_http_";

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("cannot write the object file `{}`: {source}", path.display())]
    WriteObject {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot run the linker driver `{}`: {source}", driver.to_string_lossy())]
    RunLinker {
        driver: OsString,
        source: std::io::Error,
    },
    #[error("linking failed ({status}):\n{stderr}")]
    LinkerFailed {
        status: std::process::ExitStatus,
        stderr: String,
    },
}

/// Write `object` beside `output` and link it into the executable `output`
/// against `runtime`. The linker driver is `$CC`, or `cc`.
pub fn link(object: &[u8], output: &Path, runtime: RuntimeBuild) -> Result<(), LinkError> {
    let mut object_path = output.as_os_str().to_owned();
    object_path.push(".o");
    let object_path = PathBuf::from(object_path);
    std::fs::write(&object_path, object).map_err(|source| LinkError::WriteObject {
        path: object_path.clone(),
        source,
    })?;

    // The object names the natives it calls.
    let calls_http = object
        .windows(HTTP_NATIVE.len())
        .any(|window| window == HTTP_NATIVE);
    let (archive, libs) =
        RUNTIMES[usize::from(runtime == RuntimeBuild::Checked)][usize::from(calls_http)];
    let driver = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let mut command = Command::new(&driver);
    command.arg(&object_path).arg(archive);
    command.args(libs.split_whitespace());
    // Only a little of the runtime archive is reachable from a program.
    command.arg(if cfg!(target_vendor = "apple") {
        "-Wl,-dead_strip"
    } else {
        "-Wl,--gc-sections"
    });
    command.arg("-o").arg(output);
    let result = command.output();
    let _ = std::fs::remove_file(&object_path);
    let result = result.map_err(|source| LinkError::RunLinker { driver, source })?;
    if result.status.success() {
        Ok(())
    } else {
        Err(LinkError::LinkerFailed {
            status: result.status,
            stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
        })
    }
}

/// The runtime archive of the library form (`host-lib`), with the system
/// libraries it needs.
const LIBRARY_RUNTIME: (&str, &str) = (
    env!("BAML_NATIVE_RT_LIB_ARCHIVE"),
    env!("BAML_NATIVE_RT_LIB_LINK_LIBS"),
);

/// Write `object` beside `output` and link it into the shared library
/// `output`, which a host program loads: the runtime's library form, with
/// no `main`. Only the `baml_lib_*` functions are exported, so the linker
/// still drops what nothing reaches. A spike: no `baml.http`, macOS only.
pub fn link_library(object: &[u8], output: &Path) -> Result<(), LinkError> {
    let mut object_path = output.as_os_str().to_owned();
    object_path.push(".o");
    let object_path = PathBuf::from(object_path);
    std::fs::write(&object_path, object).map_err(|source| LinkError::WriteObject {
        path: object_path.clone(),
        source,
    })?;
    let (archive, libs) = LIBRARY_RUNTIME;
    let driver = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let mut command = Command::new(&driver);
    command.arg("-dynamiclib").arg(&object_path).arg(archive);
    command.args(libs.split_whitespace());
    command.args(["-Wl,-dead_strip", "-Wl,-exported_symbol,_baml_lib_*"]);
    command.arg("-o").arg(output);
    let result = command.output();
    let _ = std::fs::remove_file(&object_path);
    let result = result.map_err(|source| LinkError::RunLinker { driver, source })?;
    if result.status.success() {
        Ok(())
    } else {
        Err(LinkError::LinkerFailed {
            status: result.status,
            stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
        })
    }
}
