#[macro_use]
extern crate log;
use colored::*;
use std::fs;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use yaml_rust::{Yaml, YamlEmitter, YamlLoader};

use clap::App;

extern crate libc;
mod utils;
extern "C" {
    fn receive_data(
        output_dir: *const libc::c_char,
        filenames: *const *const libc::c_char,
        contents: *const *const libc::c_char,
        len: libc::c_int,
        error_msg: *mut libc::c_char,
    ) -> libc::c_int;
}

fn is_poetry_enabled() -> bool {
    use std::process::Command;

    let output = Command::new("poetry").arg("--version").output();

    match output {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

fn add_gloo_lib() -> Result<(), &'static str> {
    use std::process::Command;
    if is_poetry_enabled() {
        println!("{}", "Adding gloo dependencies...".dimmed());
        let output = Command::new("poetry")
            .arg("add")
            .arg("gloo-lib@latest")
            .arg("--no-cache")
            .output()
            .expect("Failed to execute command");

        if output.status.success() {
            println!("{}", "Successfully added gloo-py to the project.".green());
            Ok(())
        } else {
            Err("Failed to add gloo-py.")
        }
    } else {
        println!("{}", "Adding gloo dependencies...".dimmed());
        let output = Command::new("pip")
            .arg("install")
            .arg("gloo-lib")
            .arg("--upgrade")
            .output()
            .expect("Failed to execute command");

        if output.status.success() {
            println!("{}", "Successfully added gloo-py to the project.".green());
            Ok(())
        } else {
            Err("Failed to add gloo-py.")
        }
    }
}

fn init_command(_init_matches: &clap::ArgMatches) {
    // Check if gloo.yaml already exists
    if Path::new("gloo.yaml").exists() {
        // At this point gloo_lib should already be in the package deps
        println!("{}", "Looks like gloo init has already been run. Delete gloo.yaml to override your existing configuration.".blue());
        return;
    }

    // Default values
    let default_output_dir = "./generated";
    let default_gloo_dir = "./gloo";

    // Ask the user for the output directory
    print!(
        "{}",
        format!(
            "Enter the output directory for generated code (default: {}): ",
            default_output_dir
        )
        .green()
    );
    io::stdout().flush().unwrap();
    let mut output_dir = String::new();
    io::stdin().read_line(&mut output_dir).unwrap();
    if output_dir.trim().is_empty() {
        output_dir = default_output_dir.to_string();
    }

    // Ask the user for the .gloo files directory
    print!(
        "{}",
        format!(
            "Enter the directory to store .gloo files (default: {}): ",
            default_gloo_dir
        )
        .green()
    );
    io::stdout().flush().unwrap();
    let mut gloo_dir = String::new();
    io::stdin().read_line(&mut gloo_dir).unwrap();
    if gloo_dir.trim().is_empty() {
        gloo_dir = default_gloo_dir.to_string();
    }

    // Create a YAML document with the user's input
    let doc = Yaml::Hash(
        vec![
            (
                Yaml::String("output_dir".to_string()),
                Yaml::String(output_dir.trim().to_string()),
            ),
            (
                Yaml::String("gloo_dir".to_string()),
                Yaml::String(gloo_dir.trim().to_string()),
            ),
        ]
        .into_iter()
        .collect(),
    );
    let mut out_str = String::new();
    {
        let mut emitter = YamlEmitter::new(&mut out_str);
        emitter.dump(&doc).unwrap(); // dump the YAML object to a String
    }

    // Write the YAML document to gloo.yaml in the current directory
    let mut file = File::create(Path::new("gloo.yaml")).unwrap();
    file.write_all(out_str.as_bytes()).unwrap();

    // Create the gloo dir
    let gloo_path = Path::new(&gloo_dir);
    if !gloo_path.exists() {
        std::fs::create_dir_all(gloo_path).unwrap();
    }

    // create a main.gloo file in the gloo dir as a text file
    let template = include_str!("../data/main.gloo.template");
    let mut file = File::create(gloo_path.join("main.gloo")).unwrap();

    // copy the contents from ./main.gloo.template into the main.gloo
    file.write_all(template.as_bytes()).unwrap();

    match add_gloo_lib() {
        Ok(_) => (),
        Err(e) => {
            println!("{}", e.red());
            return;
        }
    }

    // emit a message to the user that they can create their pipeline in the main.gloo file in a purple color
    println!(
        "{}",
        format!(
            "You can now create your LLM functions in {}/main.gloo !",
            gloo_dir
        )
        .purple()
    );
}

fn load_and_parse_yaml() -> (Yaml, PathBuf, PathBuf, PathBuf) {
    // Check if gloo.yaml exists in the current directory or any parent directory
    let mut current_dir = std::env::current_dir().unwrap();
    loop {
        let gloo_path = current_dir.join("gloo.yaml");
        if gloo_path.exists() {
            break;
        }
        if !current_dir.pop() {
            error!("gloo.yaml not found in the current directory or any parent directory. Have you run gloo init?");
            std::process::exit(1);
        }
    }

    // Load the YAML file using yaml_rust
    let mut yaml_file = File::open(current_dir.join("gloo.yaml")).unwrap();
    let mut yaml_string = String::new();
    yaml_file.read_to_string(&mut yaml_string).unwrap();
    let yaml_docs = YamlLoader::load_from_str(&yaml_string).unwrap();
    let yaml = &yaml_docs[0]; // get the first document

    let version = yaml["version"].as_str().unwrap_or("");
    if version == "" {
    } else if semver::Version::parse(version).unwrap()
        > semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap()
    {
        // Recommend the user to downgrade to the current version.
        error!(
            "{}",
            format!(
                "Your gloo version is too old. Run 'gloo update' to get version {}.",
                version
            )
            .red()
        );
        std::process::exit(1);
    }
    let output_dir = yaml["output_dir"]
        .as_str()
        .expect("gloo.yaml seems to be misconfigured. Failed to find the output_dir field.");
    let gloo_dir = yaml["gloo_dir"]
        .as_str()
        .expect("gloo.yaml seems to be misconfigured. Failed to find the gloo_dir field.");

    (
        yaml.clone(),
        fs::canonicalize(current_dir.join("gloo.yaml")).unwrap(),
        fs::canonicalize(current_dir.join(output_dir)).unwrap(),
        fs::canonicalize(current_dir.join(gloo_dir)).unwrap(),
    )
}

fn build_command(_build_matches: &clap::ArgMatches) {
    let (yaml, yaml_path, output_path, gloo_path) = load_and_parse_yaml();

    let output_dir = output_path.to_str().unwrap().to_string();
    let gloo_dir = gloo_path.to_str().unwrap().to_string();

    if !gloo_path.exists() {
        error!(
            "gloo directory not found at path: {}. Have you run gloo init?",
            gloo_path.display()
        );
        return;
    }

    fs::create_dir_all(&output_path).unwrap_or_else(|_| {
        error!(
            "Failed to create directory at path: {}",
            output_path.display()
        );
        std::process::exit(1);
    });

    // Read the files from the gloo directory
    let data = match utils::read_directory(&gloo_dir) {
        Ok(val) => val,
        Err(e) => {
            error!("{}", e);
            return;
        }
    };

    // Convert the filenames and contents into C-compatible strings
    let filenames_cstr: Vec<std::ffi::CString> = data
        .iter()
        .map(|(name, _)| std::ffi::CString::new(name.as_str()).unwrap())
        .collect();

    let contents_cstr: Vec<std::ffi::CString> = data
        .iter()
        .map(|(_, content)| std::ffi::CString::new(content.as_str()).unwrap())
        .collect();

    // Convert the CStrings into raw pointers
    let filenames_ptrs: Vec<*const libc::c_char> =
        filenames_cstr.iter().map(|cstr| cstr.as_ptr()).collect();

    let contents_ptrs: Vec<*const libc::c_char> =
        contents_cstr.iter().map(|cstr| cstr.as_ptr()).collect();

    let output_dir_cstr = std::ffi::CString::new(output_dir.clone()).unwrap();
    let output_dir_ptr = output_dir_cstr.as_ptr() as *const i8;
    let mut error_msg = [0u8; 256];
    let result = unsafe {
        receive_data(
            output_dir_ptr,
            filenames_ptrs.as_ptr(),
            contents_ptrs.as_ptr(),
            data.len() as libc::c_int,
            error_msg.as_mut_ptr() as *mut libc::c_char,
        )
    };

    // If result is 0, then the build was successful
    // update the gloo.yaml file with the current version
    if result == 0 {
        let mut yaml_hash = yaml.as_hash().unwrap().clone();
        yaml_hash.insert(
            Yaml::String("version".to_string()),
            Yaml::String(env!("CARGO_PKG_VERSION").to_string()),
        );
        let updated_yaml = Yaml::Hash(yaml_hash);
        // Write the YAML document to gloo.yaml in the current directory
        let mut file = File::create(yaml_path).unwrap();
        let mut out_str = String::new();
        {
            let mut emitter = YamlEmitter::new(&mut out_str);
            emitter.dump(&updated_yaml).unwrap(); // dump the YAML object to a String
        }
        file.write_all(out_str.as_bytes()).unwrap();
    }

    match result {
        0 => {
            // Print in green
            println!("Build complete. See: {}", output_dir.green());
        }
        _ => {
            let msg = unsafe {
                std::ffi::CStr::from_ptr(error_msg.as_ptr() as *const i8)
                    .to_string_lossy()
                    .into_owned()
            };
            error!("{}", msg);
        }
    }
}

fn update_command() -> Result<(), &'static str> {
    if cfg!(debug_assertions) {
        return Err("This command is disabled for non-release builds.");
    }
    use std::process::Command;

    println!("{}", "Updating gloo dependencies...".dimmed());

    if cfg!(target_os = "macos") {
        let output = Command::new("brew")
            .arg("tap")
            .arg("gloohq/gloo")
            .output()
            .expect("Failed to tap gloo in brew.");

        if !output.status.success() {
            return Err("Failed to tap gloo in brew.");
        }

        let output = Command::new("brew")
            .arg("update")
            .output()
            .expect("Failed to update brew");

        if !output.status.success() {
            return Err("Failed to update brew.");
        }

        let output = Command::new("brew")
            .arg("upgrade")
            .arg("gloo")
            .output()
            .expect("Failed to upgrade gloo");

        if !output.status.success() {
            return Err("Failed to upgrade gloo.");
        }
    } else if cfg!(target_os = "windows") {
        let output = Command::new("scoop")
            .arg("update")
            .output()
            .expect("Failed to update scoop");

        if !output.status.success() {
            return Err("Failed to install gloo with scoop.");
        }

        let output = Command::new("scoop")
            .arg("update")
            .arg("gloo")
            .output()
            .expect("Failed to upgrade gloo");
        if !output.status.success() {
            return Err("Failed to upgrade gloo.");
        }
    } else if cfg!(target_os = "linux") {
        let output = Command::new("sh")
            .arg("-c")
            .arg("curl -fsSL https://raw.githubusercontent.com/GlooHQ/homebrew-gloo/main/install-gloo.sh | bash")
            .output()
            .expect("Failed to execute command");

        if !output.status.success() {
            return Err("Failed to install gloo with curl.");
        }
    } else {
        return Err("Unsupported operating system for update command.");
    }

    // TODO: print out new version.
    let output = Command::new("gloo")
        .arg("-V")
        .output()
        .expect("Failed to get gloo version.");

    match add_gloo_lib() {
        Ok(_) => (),
        Err(e) => {
            println!("{}", format!("Failed to update gloo-lib: {}", e).red());
        }
    }

    let version = String::from_utf8_lossy(&output.stdout);
    println!("{}", format!("New version: {}", version).green());

    Ok(())
}

fn main() {
    pretty_env_logger::init();

    let matches = App::new("gloo")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Gloo <support@trygloo.com>")
        .about("Prisma for ML")
        .subcommand(App::new("init").about("Initializes your project to use Gloo"))
        .subcommand(App::new("build").about("Builds the project"))
        .subcommand(App::new("update").about("Updates Gloo"))
        .get_matches();

    match matches.subcommand() {
        ("init", Some(init_matches)) => init_command(init_matches),
        ("build", Some(build_matches)) => build_command(build_matches),
        ("update", Some(_)) => match update_command() {
            Ok(_) => {
                println!("{}", "Gloo has been successfully updated.".green());
            }
            Err(e) => {
                println!("{}", e.red());
                return;
            }
        },

        _ => {
            error!("Invalid command. Try `gloo --help` for more information.");
        }
    }
}
