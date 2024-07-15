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

static SAMPLE_PROJECT: include_dir::Dir =
    include_dir!("$CARGO_MANIFEST_DIR/src/cli/initial_project");

impl InitArgs {
    pub fn run(&self, caller_type: super::CallerType) -> Result<()> {
        let client_type: GeneratorOutputType = self
            .client_type
            .as_ref()
            .map_or_else(|| caller_type.into(), |x| x.into());

        // If the destination directory already contains a baml_src directory, we don't want to overwrite it.
        let dest = std::path::Path::new(&self.dest);
        let baml_src = dest.join("baml_src");
        if baml_src.exists() {
            return Err(anyhow::anyhow!(
                "Destination directory already contains a baml_src directory: {}",
                baml_src.display()
            ));
        }

        SAMPLE_PROJECT.extract(&self.dest)?;
        // Also generate a main.baml file
        let main_baml = std::path::Path::new(&self.dest)
            .join("baml_src")
            .join("generators.baml");
        std::fs::write(
            main_baml,
            format!(
                r#"
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {{
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet"
    output_type "{}"
    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"
    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The VSCode extension version should also match this version.
    version "{}"
}}
        "#,
                client_type.to_string(),
                env!("CARGO_PKG_VERSION")
            ),
        )?;

        Ok(())
    }
}
