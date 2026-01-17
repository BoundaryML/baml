use std::{
    fs,
    path::{Path, PathBuf},
};

fn main() {
    let check_mode = std::env::args().any(|arg| arg == "--check");

    let languages = discover_languages();
    let fixtures = baml_codegen_tests::FIXTURE_NAMES;

    let mut has_diff = false;

    for language in &languages {
        for fixture_name in fixtures {
            let crate_name = format!("{}_{}", language, fixture_name);
            let crate_dir = PathBuf::from("rig_tests/crates").join(&crate_name);
            let template_dir = PathBuf::from("rig_tests/crate_templates").join(language);

            if check_mode {
                if let Some(diff_info) = check_differences(&template_dir, &crate_dir, fixture_name)
                {
                    eprintln!("DIFF in {}: {}", crate_name, diff_info);
                    has_diff = true;
                }
            } else {
                generate_from_template(&template_dir, &crate_dir, fixture_name);
                println!("Generated: {}", crate_name);
            }
        }
    }

    if check_mode && has_diff {
        eprintln!("\nRun `cargo run -p baml_tools_rig` to regenerate.");
        std::process::exit(1);
    } else if !check_mode {
        // Format all generated files
        println!("\nFormatting generated files...");
        let status = std::process::Command::new("cargo")
            .args([
                "fmt",
                "--all",
                "--",
                "--config",
                "imports_granularity=Crate",
                "--config",
                "group_imports=StdExternalCrate",
            ])
            .current_dir("rig_tests/crates")
            .status()
            .expect("Failed to run cargo fmt");

        if !status.success() {
            eprintln!("Warning: cargo fmt failed");
        }
    }
}

fn discover_languages() -> Vec<String> {
    fs::read_dir("rig_tests/crate_templates")
        .unwrap_or_else(|_| panic!("Failed to read rig_tests/crate_templates"))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect()
}

fn generate_from_template(template_dir: &Path, output_dir: &Path, fixture_name: &str) {
    fs::create_dir_all(output_dir.join("src"))
        .unwrap_or_else(|_| panic!("Failed to create output directory"));

    // Always regenerate these files (they're deterministic from templates)
    for file in ["Cargo.toml", "build.rs", "src/lib.rs"] {
        let template_path = template_dir.join(format!("{}.template", file));
        let output_path = output_dir.join(file);

        let content = fs::read_to_string(&template_path)
            .unwrap_or_else(|_| panic!("Failed to read template: {}", template_path.display()));

        let rendered = content.replace("{{fixture_name}}", fixture_name);

        fs::write(&output_path, rendered)
            .unwrap_or_else(|_| panic!("Failed to write output: {}", output_path.display()));
    }

    // Copy customizable/ folder if it doesn't exist (preserve custom changes)
    let customizable_src = template_dir.join("customizable");
    let customizable_dst = output_dir.join("customizable");

    if !customizable_dst.exists() && customizable_src.exists() {
        copy_customizable_dir(&customizable_src, &customizable_dst, fixture_name)
            .unwrap_or_else(|_| panic!("Failed to copy customizable directory"));
    }
}

fn copy_customizable_dir(src: &Path, dst: &Path, fixture_name: &str) -> Result<(), std::io::Error> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);

        if path.is_file() {
            let content = fs::read_to_string(&path)?;

            // Remove .template extension if present and render templates
            let final_name = file_name.to_string_lossy();
            let final_name = final_name.strip_suffix(".template").unwrap_or(&final_name);
            let final_path = dst.join(final_name);

            let rendered = content.replace("{{fixture_name}}", fixture_name);
            fs::write(&final_path, rendered)?;
        } else if path.is_dir() {
            copy_customizable_dir(&path, &dst_path, fixture_name)?;
        }
    }

    Ok(())
}

fn check_differences(template_dir: &Path, output_dir: &Path, fixture_name: &str) -> Option<String> {
    // Only check files that are always regenerated (not test_main.py, which can be customized)
    for file in ["Cargo.toml", "build.rs", "src/lib.rs"] {
        let template_path = template_dir.join(format!("{}.template", file));
        let output_path = output_dir.join(file);

        let template_content = fs::read_to_string(&template_path)
            .unwrap_or_else(|_| panic!("Failed to read template: {}", template_path.display()))
            .replace("{{fixture_name}}", fixture_name);

        let actual_content = match fs::read_to_string(&output_path) {
            Ok(content) => content,
            Err(_) => {
                return Some(format!(
                    "{} (file missing or unreadable)",
                    output_path.display()
                ));
            }
        };

        // For Rust files, format both before comparing to ensure formatting doesn't cause false positives
        let (template_formatted, actual_formatted) = if file.ends_with(".rs") {
            (
                format_rust_code(&template_content),
                format_rust_code(&actual_content),
            )
        } else {
            (template_content.clone(), actual_content.clone())
        };

        if template_formatted != actual_formatted {
            let template_lines = template_formatted.lines().count();
            let actual_lines = actual_formatted.lines().count();
            let template_bytes = template_formatted.len();
            let actual_bytes = actual_formatted.len();

            return Some(format!(
                "{} ({} lines vs {}, {} bytes vs {})",
                file, template_lines, actual_lines, template_bytes, actual_bytes
            ));
        }
    }
    None
}

/// Format Rust code using rustfmt
fn format_rust_code(code: &str) -> String {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    let mut child = Command::new("rustfmt")
        .args([
            "--edition",
            "2021",
            "--config",
            "imports_granularity=Crate",
            "--config",
            "group_imports=StdExternalCrate",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn rustfmt");

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(code.as_bytes())
            .expect("Failed to write to rustfmt stdin");
    }

    let output = child
        .wait_with_output()
        .expect("Failed to wait for rustfmt");

    if output.status.success() {
        String::from_utf8_lossy(&output.stdout).to_string()
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "rustfmt failed with status: {}\nstderr: {}",
            output.status, stderr
        );
    }
}
