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
                if has_differences(&template_dir, &crate_dir, fixture_name) {
                    eprintln!("DIFF: {}", crate_name);
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

fn has_differences(template_dir: &Path, output_dir: &Path, fixture_name: &str) -> bool {
    // Only check files that are always regenerated (not test_main.py, which can be customized)
    for file in ["Cargo.toml", "build.rs", "src/lib.rs"] {
        let template_path = template_dir.join(format!("{}.template", file));
        let output_path = output_dir.join(file);

        let expected = fs::read_to_string(&template_path)
            .unwrap_or_else(|_| panic!("Failed to read template: {}", template_path.display()))
            .replace("{{fixture_name}}", fixture_name);

        let actual = fs::read_to_string(&output_path).unwrap_or_default();

        if expected != actual {
            return true;
        }
    }
    false
}
