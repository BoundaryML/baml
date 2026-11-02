//! Ruby's fixture gates keep each minitest file in its own process, and check
//! the entire generated package and test overlay against the gem's shipped RBI.
use std::{env, fs, path::PathBuf, process::Command};

use crate::{fixture_path, run_command, workspace_root_from_manifest};

pub fn minitest(fixture: &str) {
    let generated = fixture_path(fixture, "generated");
    let mut tests: Vec<_> = fs::read_dir(&generated)
        .expect("generated Ruby fixture")
        .map(|entry| entry.expect("generated fixture entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("test_") && name.ends_with(".rb"))
        })
        .collect();
    tests.sort();
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let runtime = workspace_root_from_manifest(&manifest).join("sdks/ruby/bridge_ruby/lib");
    // Fixtures without ports still prove the generated SDK loads.
    if tests.is_empty() {
        tests.push(PathBuf::from("-e0"));
    }
    for test in tests {
        run_command(
            Command::new("ruby")
                .args(["-S", "bundle", "exec", "ruby", "-I"])
                .arg(&runtime)
                .arg("-I")
                .arg(&generated)
                .arg("-rbaml_sdk")
                .arg(&test)
                .current_dir(&generated),
            &format!("Ruby {fixture} {}", test.display()),
        );
    }
}

/// Also used by the single negative consumer, which must reject a wrong type.
pub fn sorbet_command(fixture: &str) -> Command {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let workspace = workspace_root_from_manifest(&manifest);
    let target = env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| workspace.join("target"), |path| workspace.join(path));
    let mut command = Command::new("ruby");
    command
        .args([
            "-S",
            "bundle",
            "exec",
            "srb",
            "tc",
            "--no-config",
            "--color=never",
            "--no-error-sections",
            "--no-error-count",
        ])
        .env("BUNDLE_GEMFILE", manifest.join("test/static/Gemfile"))
        .env("BUNDLE_PATH", target.join("ruby-sorbet-static-bundle"))
        .env("BUNDLE_FROZEN", "true")
        .arg(workspace.join("sdks/ruby/bridge_ruby/rbi/baml.rbi"))
        .arg(manifest.join("test/static/minitest.rbi"))
        .arg(".")
        .current_dir(fixture_path(fixture, "generated"));
    command
}

/// Expand a Ruby check, preserving the static checker's supported platforms.
#[macro_export]
#[doc(hidden)]
macro_rules! __ruby_sorbet_gate {
    (on, minitest, $body:block) => {
        #[test]
        fn minitest() $body
    };
    (on, sorbet, $body:block) => {
        #[test]
        #[cfg_attr(
            not(all(
                any(target_arch = "x86_64", target_arch = "aarch64"),
                any(target_os = "macos", all(target_os = "linux", target_env = "gnu"))
            )),
            ignore = "Sorbet 0.6.13506 binaries require x86_64/arm64 macOS or x86_64/aarch64 glibc Linux; runtime tests still run"
        )]
        fn sorbet() $body
    };
    (later, $name:ident, $body:block) => {
        #[test]
        #[ignore = "generated Ruby API not complete enough for this fixture yet"]
        fn $name() $body
    };
}

#[macro_export]
macro_rules! ruby_sorbet_test_suite {
    ( $( fixture $name:ident { minitest: $minitest:ident, sorbet: $sorbet:ident } )+ ) => {
        $crate::setup_guard!("SDK_TEST_RUBY_SORBET_SETUP");
        $crate::fixture_manifest!( $( $name ),+ );

        $(
            mod $name {
                $crate::__ruby_sorbet_gate!($minitest, minitest, {
                    $crate::ruby_sorbet::minitest(stringify!($name));
                });
                $crate::__ruby_sorbet_gate!($sorbet, sorbet, {
                    $crate::run_command(
                        &mut $crate::ruby_sorbet::sorbet_command(stringify!($name)),
                        concat!("Ruby ", stringify!($name), " srb tc"),
                    );
                });
            }
        )+
    };
}

pub use crate::ruby_sorbet_test_suite as test_suite;
