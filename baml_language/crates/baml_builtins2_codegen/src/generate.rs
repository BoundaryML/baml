//! File generation shared by Cargo build scripts and the standalone host tool.

use std::{error::Error, fs, path::Path};

/// The crate that will compile the generated Rust files.
#[derive(Clone, Copy, Debug)]
pub enum BuiltinCrate {
    BexVm,
    BexVmTypes,
    SysOps,
    SysTypes,
}

/// Generate one crate's builtin bindings into its own output directory.
///
/// Some crates use the same filenames for different bindings, so callers must
/// keep their output directories separate. The files depend only on the
/// generator and its embedded standard-library sources.
pub fn generate(consumer: BuiltinCrate, out_dir: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(out_dir)?;
    if matches!(consumer, BuiltinCrate::BexVm) {
        for (package, file) in [
            ("baml", "nativefunctions_generated.rs"),
            ("ai", "aifunctions_generated.rs"),
            ("reflect", "reflectfunctions_generated.rs"),
        ] {
            let (vm_builtins, _io_builtins, class_defs) =
                crate::extract_native_builtins_for(package)?;
            let code = crate::generate_native_trait_for(package, &vm_builtins, &class_defs);
            fs::write(out_dir.join(file), code)?;
        }
        return Ok(());
    }

    let (_vm_builtins, io_builtins, class_defs) = crate::extract_native_builtins()?;
    match consumer {
        BuiltinCrate::BexVmTypes => {
            fs::write(
                out_dir.join("sys_op_generated.rs"),
                crate::generate_sys_op_enum(&io_builtins),
            )?;
            fs::write(
                out_dir.join("errors_generated.rs"),
                crate::generate_error_enums(&class_defs),
            )?;
            fs::write(
                out_dir.join("panics_generated.rs"),
                crate::generate_panic_enums(&class_defs),
            )?;
        }
        BuiltinCrate::SysOps => {
            fs::write(
                out_dir.join("io_generated.rs"),
                crate::generate_io_traits(&io_builtins, &class_defs, "sys_types::generated"),
            )?;
            fs::write(
                out_dir.join("io_adapter.rs"),
                crate::generate_io_adapter(&io_builtins, &class_defs, "sys_types::generated"),
            )?;
        }
        BuiltinCrate::SysTypes => {
            fs::write(
                out_dir.join("io_generated.rs"),
                crate::generate_io_structs(&io_builtins, &class_defs),
            )?;
            fs::write(
                out_dir.join("runtime_io.rs"),
                crate::generate_runtime_io(&io_builtins, &class_defs, "super::generated"),
            )?;
        }
        BuiltinCrate::BexVm => unreachable!("VM bindings were generated above"),
    }
    Ok(())
}
