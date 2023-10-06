extern crate cc;
extern crate which;

use std::env;
use std::path::Path;
use walkdir::WalkDir;
use which::which;

// Build the C++ code into a static library
fn main() {
    // Check if ccache is available on the system
    if let Ok(ccache_path) = which("ccache") {
        env::set_var("CC", &ccache_path);
        // print out the path to ccache
        println!("cargo:warning=Using ccache at {}", ccache_path.display());
    }

    let mut build = cc::Build::new();
    build.cpp(true).warnings(true);

    let cpp_path = Path::new("cpp_src");
    if cpp_path.exists() && cpp_path.is_dir() {
        for entry in WalkDir::new(cpp_path) {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() && path.extension().unwrap_or_default() == "cc" {
                build.file(path);
            }
        }
    }

    #[cfg(debug_assertions)]
    build.flag_if_supported("-O0").flag_if_supported("-g");

    #[cfg(not(debug_assertions))]
    build.flag_if_supported("-O2");

    build.include(cpp_path);

    // Determine if we're targeting MSVC
    let target = env::var("TARGET").unwrap();
    if target.contains("msvc") {
        // Flags for MSVC
        build.flag("/W4").flag("/WX").flag("/std:c++20").flag("/EHsc");
    } else {
        // If mac, set MACOSX_DEPLOYMENT_TARGET: 11.0
        if target.contains("apple") {
            println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET=11.0");
        }
        // Flags for GCC/Clang
        build.flag("-Wall").flag("-Wextra").flag("-Werror").flag("-std=c++2a");
    }

    build.compile("program");

    println!("cargo:rerun-if-changed=cpp_src/");
}
