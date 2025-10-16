use std::collections::HashMap;

use baml_runtime::BamlRuntime;
use internal_baml_core::feature_flags::FeatureFlags;

use super::*;
use crate::panic::ffi_safe::{ffi_safe_cstring, ffi_safe_ptr};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[no_mangle]
pub extern "C" fn version() -> *const libc::c_char {
    ffi_safe_cstring(|| match CString::new(VERSION) {
        Ok(version) => Ok(version.into_raw() as *const libc::c_char),
        Err(_) => Err("Version string contains null bytes".to_string()),
    })
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn create_baml_runtime(
    root_path: *const libc::c_char,
    src_files_json: *const libc::c_char,
    env_vars_json: *const libc::c_char,
) -> *const libc::c_void {
    ffi_safe_ptr(|| -> Result<*const libc::c_void, String> {
        // Parse src_files JSON
        let src_files_str = unsafe {
            CStr::from_ptr(src_files_json)
                .to_str()
                .map_err(|e| format!("Invalid UTF-8 in src_files_json: {e}"))?
        };
        let src_files = serde_json::from_str::<HashMap<String, String>>(src_files_str)
            .map_err(|e| format!("Failed to parse src_files JSON: {e}"))?;

        // Parse env_vars JSON
        let env_vars_str = unsafe {
            CStr::from_ptr(env_vars_json)
                .to_str()
                .map_err(|e| format!("Invalid UTF-8 in env_vars_json: {e}"))?
        };
        let env_vars = serde_json::from_str::<HashMap<String, String>>(env_vars_str)
            .map_err(|e| format!("Failed to parse env_vars JSON: {e}"))?;

        // Parse root_path
        let root_path_str = unsafe {
            CStr::from_ptr(root_path)
                .to_str()
                .map_err(|e| format!("Invalid UTF-8 in root_path: {e}"))?
        };

        // Create runtime
        let runtime = BamlRuntime::from_file_content(
            root_path_str,
            &src_files,
            env_vars,
            FeatureFlags::new(),
        )
        .map_err(|e| format!("Failed to create BAML runtime: {e}"))?;

        Ok(Box::into_raw(Box::new(runtime)) as *const libc::c_void)
    })
}

#[no_mangle]
pub extern "C" fn destroy_baml_runtime(runtime: *const libc::c_void) {
    if !runtime.is_null() {
        unsafe {
            let _ = Box::from_raw(runtime as *mut BamlRuntime);
        }
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn invoke_runtime_cli(args: *const *const libc::c_char) -> libc::c_int {
    // Safety: We assume `args` is a valid pointer to a null-terminated array of C strings.
    let args_vec = unsafe {
        // Ensure the pointer itself is not null.
        if args.is_null() {
            Vec::new()
        } else {
            let mut vec = Vec::new();
            let mut i = 0;
            // Iterate until a null pointer is encountered.
            while !(*args.add(i)).is_null() {
                let c_str = CStr::from_ptr(*args.add(i));
                // Convert to Rust String (lossy conversion handles non-UTF8 gracefully).
                vec.push(c_str.to_string_lossy().into_owned());
                i += 1;
            }
            vec
        }
    };
    match baml_cli::run_cli(
        args_vec,
        baml_runtime::RuntimeCliDefaults {
            output_type: baml_types::GeneratorOutputType::Go,
        },
    ) {
        Ok(exit_code) => exit_code.into(),
        Err(e) => {
            baml_log::error!("{}", e);
            1
        }
    }
}
