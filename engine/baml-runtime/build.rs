use anyhow::{Context, Result};
use std::env;
use std::process::Command;

fn check_ruby_version() -> Result<()> {
    // Run the 'ruby -v' command to get the Ruby version
    let output = Command::new("ruby").arg("-v").output().context(format!(
        "Failed while running 'ruby -v': Ruby does not appear to be installed"
    ))?;

    // Extract the version number from the string
    // Typical output: "ruby 3.0.0p0 (2020-12-25 revision 95aff21468) [x86_64-darwin20]"
    let version_str = String::from_utf8_lossy(&output.stdout);
    let version = version_str.split_ascii_whitespace().nth(1).ok_or_else(|| {
        anyhow::anyhow!("Failed to extract Ruby version string from 'ruby -v' output")
    })?;

    if version.chars().next().map_or(false, |c| c.is_ascii_digit()) && version >= "3.1" {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "BAML must be built with Ruby >= 3.1, but found Ruby {version}"
        ))
    }
}

fn check_clang_support() -> Result<()> {
    if std::env::var("CARGO_CFG_TARGET_ARCH").ok() != Some("wasm32".to_string()) {
        return Ok(());
    }

    // Run the 'clang --version' command to get the Clang version
    let output = Command::new("clang")
        .arg("--print-targets")
        .output()
        .context(format!("Failed while running 'clang --print-targets'"))?;

    // Extract the version number from the string
    let target_list = String::from_utf8_lossy(&output.stdout);
    let target_list = target_list
        .lines()
        .filter_map(|line| line.split_once(" - "))
        .map(|(target, description)| (target.trim(), description.trim()))
        .map(|(target, _)| target)
        .collect::<Vec<_>>();

    if !target_list.contains(&"wasm32") {
        // See https://github.com/briansmith/ring/issues/1824
        return Err(
          anyhow::anyhow!("clang does not support the wasm32 target: clang --print-targets returned {:?}", target_list)
            .context("BAML must be built with Clang with wasm32 target support - you need to 'brew install clang'")
        );
    }

    Ok(())
}

#[allow(dead_code)]
fn print_env() {
    let mut env = std::env::vars().collect::<Vec<_>>();
    env.sort();
    let env = env;

    println!("Environment variables:");
    for (key, value) in &env {
        println!("{}={}", key, value);
    }
}

fn main() {
    let baml_build = env::var("BAML_BUILD_HELP").unwrap_or_else(|_| "AUTO".to_string());

    if baml_build != "AUTO" {
        return;
    }

    println!("Running build checks - set BAML_BUILD_HELP=off to disable.");

    let mut errors = vec![];

    if let Err(e) = check_ruby_version().context("Please install mise and direnv to build BAML") {
        errors.push(e);
    }

    if let Err(e) = check_clang_support().context("Please install clang and direnv to build BAML") {
        errors.push(e);
    }

    if errors.is_empty() {
        return;
    }

    println!("");
    println!("Please install mise and direnv to build BAML (instructions: https://www.notion.so/gloochat/To-build-BAML-0e9e3e9b583e40fb8fb040505b24d65f )");
    println!("");
    println!("The following errors occurred during build checks:");
    for error in errors {
        println!("{:#}", error);
    }

    // println!();
    // print_env();
    // println!();

    if env::var("RA_RUSTC_WRAPPER").ok() == Some("1".to_string()) {
        // NB(sam): for some reason, on my machine, rust-analyzer doesn't appear to reflect changes
        // made in .zshrc/.zprofile/.bashrc/.bash_profile. It's clearly _running_ them, because I see
        // stuff like STARSHIP_SESSION getting set, but I can't figure out how to point it at my own
        // Ruby install. Best solution is to just disable it, and let `cargo check` check against
        // the system Ruby, so the result is that rust-analyzer will soft fail on... I guess Linux?
        // which is a compromise I can live with for now.
        println!("Running inside rust-analyzer - will not induce build failure");
        return;
    }

    // comment this out - might cause a build break
    // panic!("Build checks failed");
}
