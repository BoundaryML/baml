use baml::{parse_configuration, parse_schema};
use colored::*;
use log::{error, info, warn};
use std::path::PathBuf;

pub fn build(baml_dir: &Option<String>) -> Result<(), Option<&'static str>> {
    let src_dir = baml_dir.as_deref().unwrap_or(".");
    let abs_src_dir = PathBuf::from(src_dir).canonicalize();

    if let Err(_) = abs_src_dir {
        error!("Failed to find BAML project directory: {}", src_dir.bold());
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
    Ok(())
}
