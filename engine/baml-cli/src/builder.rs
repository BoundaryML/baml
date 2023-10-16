use baml::{parse_configuration, parse_schema};
use colored::*;
use log::{error, info, warn};
use std::path::PathBuf;

// Walk up a directory until you find a directory named: baml_src
fn default_baml_dir() -> Result<PathBuf, &'static str> {
    let mut current_dir = std::env::current_dir().unwrap();
    loop {
        let baml_dir = current_dir.join("baml_src");
        if baml_dir.exists() {
            return Ok(baml_dir);
        }
        if !current_dir.pop() {
            break;
        }
    }
    Err("Failed to find a directory named: baml_src")
}

pub fn build(baml_dir: &Option<String>) -> Result<(), Option<&'static str>> {
    let src_dir = match baml_dir {
        Some(dir) => PathBuf::from(dir),
        None => match default_baml_dir() {
            Ok(dir) => dir,
            Err(err) => {
                return Err(Some(err));
            }
        },
    };
    let abs_src_dir = src_dir.canonicalize();

    if let Err(_) = abs_src_dir {
        error!("Dir not found: {}", src_dir.to_string_lossy().bold());
        return Err(None);
    }
    let baml_dir = abs_src_dir.unwrap();

    // Find a main.baml file
    let main_baml = baml_dir.join("main.baml");
    if !main_baml.exists() {
        error!(
            "Failed to find {} file at {}\nBAML projects require: {}",
            "main.baml".bold(),
            main_baml.to_string_lossy().bold(),
            "<baml_dir>/main.baml".bold()
        );
        return Err(None);
    }

    info!("Building: {}", baml_dir.to_string_lossy().bold());

    // Read the main.baml file
    let main_baml_contents = std::fs::read_to_string(&main_baml);
    if let Err(_) = main_baml_contents {
        error!("Failed to read {}", main_baml.to_string_lossy().bold());
        return Err(None);
    }
    let main_baml_contents = main_baml_contents.unwrap();
    match parse_configuration(&main_baml_contents) {
        Ok(config) => {
            info!("Configuration: {:?}", config);
        }
        Err(err) => {
            error!("Failed to parse {}", main_baml.to_string_lossy().bold());
            println!(
                "{}",
                err.warnings_to_pretty_string("main.baml", &main_baml_contents)
            );
            println!("{}", err.to_pretty_string("main.baml", &main_baml_contents));
            return Err(None);
        }
    }
    Ok(())
}
