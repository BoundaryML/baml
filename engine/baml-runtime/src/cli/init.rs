use anyhow::Result;
use include_dir::include_dir;
use internal_baml_core::configuration::GeneratorOutputType;

use super::LanguageClientType;

#[derive(clap::Args, Debug)]
pub struct InitArgs {
    #[arg(
        long,
        help = "where to initialize the BAML project (default: current directory)",
        default_value = "."
    )]
    dest: String,

    #[arg(long, help = "type of BAML client to generate")]
    client_type: Option<LanguageClientType>,
}

impl InitArgs {
    pub fn run(&self, caller_type: super::CallerType) -> Result<()> {
        let client_type: GeneratorOutputType = self
            .client_type
            .as_ref()
            .map_or_else(|| caller_type.into(), |x| x.into());

        // Staticly copy
        static SAMPLE_PROJECT: include_dir::Dir =
            include_dir!("$CARGO_MANIFEST_DIR/src/cli/initial_project");

        // Copy everything from the sample project to the destination
        let file_prefix = format!("{}/src/cli/initial_project", env!("CARGO_MANIFEST_DIR"));
        for entry in SAMPLE_PROJECT.files() {
            let path = entry.path();
            let dest =
                std::path::Path::new(&self.dest).join(path.strip_prefix(&file_prefix).unwrap());
            std::fs::create_dir_all(dest.parent().unwrap())?;
            std::fs::write(dest, entry.contents())?;
        }

        // Also generate a main.baml file
        let main_baml = std::path::Path::new(&self.dest).join("generators.baml");
        std::fs::write(
            main_baml,
            format!(
                r#"
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {{
    // Valid values: "typescript", "python-pydantic", "ruby"
    output_type "{}"
    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"
}}
        "#,
                client_type.to_string(),
            ),
        )?;

        Ok(())
    }
}
