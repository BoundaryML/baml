pub fn init_command() -> Result<(), CliError> {
    let baml_src = Path::new("baml_src");
    if !baml_src.exists() {
        std::fs::create_dir_all(baml_src).unwrap();
    }

    Ok(())
}
