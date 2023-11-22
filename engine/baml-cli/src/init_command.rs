use std::path::Path;

use crate::errors::CliError;

pub fn init_command() -> Result<(), CliError> {
    let baml_src = Path::new("baml_src");
    if !baml_src.exists() {
        std::fs::create_dir_all(baml_src).unwrap();
    }

    // copy the files from the sample/ directory to the baml_src/ directory
    let sample_dir = Path::new("src/sample");
    for entry in sample_dir.read_dir().unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let file_name = path.file_name().unwrap().to_str().unwrap();
        let dest_path = baml_src.join(file_name);
        std::fs::copy(path, dest_path).unwrap();
    }

    Ok(())
}
