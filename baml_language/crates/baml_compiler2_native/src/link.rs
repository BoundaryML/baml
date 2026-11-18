//! Links a compiled object with the runtime archive into an executable,
//! using the system C compiler as the linker driver.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

/// The runtime archive this build of the compiler was built with, and the
/// system libraries the archive needs, as linker arguments.
const RUNTIME: (&str, &str) = (
    env!("BAML_NATIVE_RT_ARCHIVE"),
    env!("BAML_NATIVE_RT_LINK_LIBS"),
);
/// The same runtime built with `baml.http`. A TLS library sets itself up
/// when a program that links it starts, and brings system frameworks to
/// load, whether or not a request is ever sent, and the linker cannot drop
/// either. So only a program that calls a native of `baml.http` is linked
/// against this one.
const RUNTIME_WITH_HTTP: (&str, &str) = (
    env!("BAML_NATIVE_RT_HTTP_ARCHIVE"),
    env!("BAML_NATIVE_RT_HTTP_LINK_LIBS"),
);
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

/// Write `object` beside `output` and link it into the executable `output`.
/// The linker driver is `$CC`, or `cc`.
pub fn link(object: &[u8], output: &Path) -> Result<(), LinkError> {
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
    let (archive, libs) = if calls_http {
        RUNTIME_WITH_HTTP
    } else {
        RUNTIME
    };
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
