//! Links a compiled object with the runtime archive into an executable,
//! using the system C compiler as the linker driver.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

/// The runtime archive this build of the compiler was built with.
const RUNTIME_ARCHIVE: &str = env!("BAML_NATIVE_RT_ARCHIVE");
/// The system libraries the runtime archive needs, as linker arguments.
const RUNTIME_LINK_LIBS: &str = env!("BAML_NATIVE_RT_LINK_LIBS");

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

    let driver = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let mut command = Command::new(&driver);
    command.arg(&object_path).arg(RUNTIME_ARCHIVE);
    command.args(RUNTIME_LINK_LIBS.split_whitespace());
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
