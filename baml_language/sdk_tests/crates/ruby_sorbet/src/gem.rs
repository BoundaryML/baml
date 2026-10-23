use std::process::Command;

#[test]
fn clean_app() {
    let arch = std::env::consts::ARCH;
    let target = if cfg!(target_os = "macos") {
        format!("{arch}-apple-darwin")
    } else if cfg!(target_os = "windows") {
        format!("{arch}-pc-windows-msvc")
    } else if cfg!(target_os = "linux") {
        let libc = if cfg!(target_env = "musl") {
            "musl"
        } else {
            "gnu"
        };
        format!("{arch}-unknown-linux-{libc}")
    } else {
        panic!("unsupported Ruby bridge platform");
    };
    let engine = sdk_test_harness_runner::engine_library();
    let cli = engine
        .parent()
        .expect("engine directory")
        .join(if cfg!(windows) {
            "baml-cli.exe"
        } else {
            "baml-cli"
        });
    let mut command = Command::new("ruby");
    command
        .args(["-S", "bundle", "exec", "ruby", "test/gem_smoke.rb"])
        .arg(&engine)
        .arg(target)
        .arg(cli)
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    // Same hosts as the Sorbet gate; setup installs the checker only there.
    let sorbet = cfg!(all(
        any(target_arch = "x86_64", target_arch = "aarch64"),
        any(
            target_os = "macos",
            all(target_os = "linux", target_env = "gnu")
        )
    ));
    if sorbet {
        let target_dir = engine
            .parent()
            .and_then(|profile| profile.parent())
            .expect("target directory");
        command.arg(target_dir.join("ruby-sorbet-static-bundle"));
    }
    let output = sdk_test_harness_runner::run_command(
        &mut command,
        "installed baml-bridge gem in a clean app",
    );
    sdk_test_harness_runner::assert_stdout_contains(
        &output,
        "Installed baml-bridge gem: clean app returned hello world",
    );
    if sorbet {
        sdk_test_harness_runner::assert_stdout_contains(
            &output,
            "Sorbet accepted the clean app with the installed RBI",
        );
    }
    print!("{}", String::from_utf8_lossy(&output.stdout));
}
