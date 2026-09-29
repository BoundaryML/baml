//! The single-level stdlib artifact embedded by `bex_project`.
//!
//! Both Cargo and external build systems call this producer. Keep its format,
//! optimization level, and compatibility key shared with the runtime consumer.

use std::{fs, io, path::Path};

use super::{OptLevel, build_stdlib_prefix};

pub const OPT_LEVEL: OptLevel = OptLevel::One;
pub const FILE_NAME: &str = "stdlib_prefix.borsh";

pub fn artifact_key() -> String {
    let opt_level = match OPT_LEVEL {
        OptLevel::Zero => "zero",
        OptLevel::One => "one",
        OptLevel::Two => "two",
    };
    format!(
        "bex-project-stdlib-prefix-v2:version={}:channel={}:opt={opt_level}",
        baml_version::CANONICAL_VERSION,
        baml_version::CHANNEL,
    )
}

/// Compile and write the runtime stdlib artifact using this compiler build.
///
/// Build systems must track the compiler, serialization types, embedded stdlib,
/// and version/configuration inputs; the compatibility key alone does not
/// identify every change to those inputs.
pub fn generate(out_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(out_dir)?;
    let prefix = build_stdlib_prefix(OPT_LEVEL);
    let artifact = (artifact_key(), prefix.interfaces, prefix.program);
    let bytes = borsh::to_vec(&artifact)?;
    fs::write(out_dir.join(FILE_NAME), bytes)
}
