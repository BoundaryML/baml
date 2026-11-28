//! Builds the runtime that native programs link against into a static
//! archive, and tells the crate where it is and which system libraries it
//! needs at link time. Also writes the table of standard library natives the
//! runtime implements, which the crate includes.
//!
//! The runtime is the workspace package `baml_native_rt`, built by a nested
//! `cargo` with its own profile and target directory: programs need it
//! optimized, aborting on panic and with frame pointers whatever profile the
//! compiler itself is built with.

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

/// The line `rustc --print native-static-libs` reports the libraries on.
const NATIVE_LIBS_NOTE: &str = "native-static-libs:";

/// The profile the runtime is built with, defined on the nested command line
/// so that it lives beside the one build that uses it.
const PROFILE: &str = "native-rt";
const PROFILE_SETTINGS: [&str; 9] = [
    "inherits=\"release\"",
    "opt-level=3",
    "codegen-units=1",
    "lto=false",
    "panic=\"abort\"",
    "strip=false",
    "debug=false",
    "debug-assertions=false",
    "overflow-checks=false",
];

/// Set by an enclosing `cargo` for its own compiler invocations and for
/// this script; the nested build must not inherit them. The last says the
/// compiler is built with debug assertions, which the runtime's own build
/// script would read as the runtime being built with them.
const OUTER_ONLY: [&str; 6] = [
    "CARGO_ENCODED_RUSTFLAGS",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTC_WRAPPER",
    "CLIPPY_ARGS",
    "CARGO_TARGET_DIR",
    "CARGO_CFG_DEBUG_ASSERTIONS",
];

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("set by cargo"));

    let runtime_dir = manifest_dir.join("../baml_native_rt");
    for input in ["src", "natives", "build.rs", "Cargo.toml"] {
        println!(
            "cargo:rerun-if-changed={}",
            runtime_dir.join(input).display()
        );
    }
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("../baml_builtins2/baml_std").display()
    );

    // The runtime is built four times: without `baml.http` for the
    // programs that call none of it and with it for the others, and each
    // of those again with the checks a test build links.
    // A fifth is the library form a host program loads (`host-lib`): no
    // `main`, lean, without `baml.http`.
    for (variant, with_http, checked, library) in [
        ("", false, false, false),
        ("_HTTP", true, false, false),
        ("_CHECKED", false, true, false),
        ("_HTTP_CHECKED", true, true, false),
        ("_LIB", false, false, true),
    ] {
        let (archive, native_libs) =
            build_runtime(&runtime_dir, &out_dir, with_http, checked, library);
        println!(
            "cargo:rustc-env=BAML_NATIVE_RT{variant}_ARCHIVE={}",
            archive.display()
        );
        println!("cargo:rustc-env=BAML_NATIVE_RT{variant}_LINK_LIBS={native_libs}");
    }

    let implemented = baml_builtins2_codegen::read_native_lists(&runtime_dir.join("natives"))
        .unwrap_or_else(|error| panic!("{error}"));
    let (vm, io, classes) =
        baml_builtins2_codegen::extract_native_builtins().unwrap_or_else(|error| panic!("{error}"));
    let table = baml_builtins2_codegen::generate_native_table(&vm, &io, &classes, &implemented)
        .unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(out_dir.join("native_table.rs"), table)
        .expect("the output directory is writable");
}

/// Where the nested build keeps its artifacts: beside the profile
/// directories of the enclosing build, so that every profile of the compiler
/// shares one build of the runtime.
fn runtime_target_dir(out_dir: &Path) -> PathBuf {
    // `OUT_DIR` is `<target>[/<triple>]/<profile>/build/<package>-<hash>/out`.
    out_dir
        .ancestors()
        .find(|dir| dir.file_name().is_some_and(|name| name == "build"))
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map_or_else(|| out_dir.join("native-rt"), |dir| dir.join("native-rt"))
}

/// Build the runtime archive, with `baml.http` or without and with its
/// checks or without, and return it with the system libraries it needs as
/// linker arguments.
fn build_runtime(
    runtime_dir: &Path,
    out_dir: &Path,
    with_http: bool,
    checked: bool,
    library: bool,
) -> (PathBuf, String) {
    let cargo = env::var_os("CARGO").expect("set by cargo");
    let target = env::var("TARGET").expect("set by cargo");
    let host = env::var("HOST").expect("set by cargo");
    let target_dir = runtime_target_dir(out_dir);

    let mut command = Command::new(cargo);
    command.current_dir(runtime_dir);
    for setting in PROFILE_SETTINGS {
        command.args(["--config", &format!("profile.{PROFILE}.{setting}")]);
    }
    command
        .args(["rustc", "--package", "baml_native_rt", "--lib"])
        .args(["--crate-type", "staticlib"])
        .args(["--profile", PROFILE])
        .arg("--target-dir")
        .arg(&target_dir);
    if !with_http {
        command.arg("--no-default-features");
    }
    if checked {
        command.args(["--features", "checks"]);
    }
    if library {
        command.args(["--features", "host-lib"]);
    }
    let mut artifact_dir = target_dir;
    if target != host {
        command.args(["--target", &target]);
        artifact_dir.push(&target);
    }
    command.args(["--", "--print", "native-static-libs"]);
    for variable in OUTER_ONLY {
        command.env_remove(variable);
    }
    // The collector walks the stack through frame records.
    command.env("RUSTFLAGS", "-C force-frame-pointers=yes");

    let output = command.output().expect("cargo runs");
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "building the native runtime failed:\n{diagnostics}"
    );

    // Every build leaves its archive under one name, so each is kept
    // under a name of its own.
    let built = artifact_dir.join(PROFILE).join("libbaml_native_rt.a");
    assert!(
        built.is_file(),
        "the nested build left no archive at `{}`",
        built.display()
    );
    let archive = built.with_file_name(match (with_http, checked) {
        _ if library => "libbaml_native_rt_lib.a",
        (false, false) => "libbaml_native_rt_plain.a",
        (true, false) => "libbaml_native_rt_http.a",
        (false, true) => "libbaml_native_rt_plain_checked.a",
        (true, true) => "libbaml_native_rt_http_checked.a",
    });
    std::fs::copy(&built, &archive).expect("the target directory is writable");
    // `rustc` names the libraries only when it compiles the archive, so they
    // are kept beside it for the builds that find it fresh.
    let libs_file = archive.with_extension("libs");
    let native_libs = match diagnostics
        .lines()
        .find_map(|line| line.split_once(NATIVE_LIBS_NOTE))
    {
        Some((_, libs)) => {
            let libs = libs.trim().to_owned();
            std::fs::write(&libs_file, &libs).expect("the target directory is writable");
            libs
        }
        None => std::fs::read_to_string(&libs_file).unwrap_or_else(|error| {
            panic!(
                "the runtime archive is fresh but `{}` is missing: {error}",
                libs_file.display()
            )
        }),
    };
    (archive, native_libs)
}
