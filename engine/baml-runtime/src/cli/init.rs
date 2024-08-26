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

        let main_baml_content = generate_main_baml_content(&client_type);
        std::fs::write(main_baml, main_baml_content)?;

        Ok(())
    }
}

fn generate_main_baml_content(client_type: &GeneratorOutputType) -> String {
    let default_mode = client_type.recommended_default_client_mode();

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
    // The BAML VSCode extension version should also match this version.
    version "{}"
    // Valid values: "sync", "async"
    // This controls what `b.FunctionName()` will be (sync or async).
    // Regardless of this setting, you can always explicitly call either of the following:
    // - b.sync.FunctionName()
    // - b.async_.FunctionName() (note the underscore to avoid a keyword conflict)
    default_client_mode {}
}}
       "#,
        client_type.to_string(),
        env!("CARGO_PKG_VERSION"),
        default_mode.to_string()
    )
    .trim_end()
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_generate_content_pydantic() {
        assert_eq!(
            generate_main_baml_content(&GeneratorOutputType::PythonPydantic),
            format!(r#"
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {{
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet"
    output_type "python/pydantic"
    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"
    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The BAML VSCode extension version should also match this version.
    version "{}"
    // Valid values: "sync", "async"
    // This controls what `b.FunctionName()` will be (sync or async).
    // Regardless of this setting, you can always explicitly call either of the following:
    // - b.sync.FunctionName()
    // - b.async_.FunctionName() (note the underscore to avoid a keyword conflict)
    default_client_mode sync
}}
        "#, env!("CARGO_PKG_VERSION"))
            .trim_end()
        );
    }

    #[test]
    fn test_generate_content_typescript() {
        assert_eq!(
            generate_main_baml_content(&GeneratorOutputType::Typescript),
            format!(r#"
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {{
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet"
    output_type "typescript"
    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"
    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The BAML VSCode extension version should also match this version.
    version "{}"
    // Valid values: "sync", "async"
    // This controls what `b.FunctionName()` will be (sync or async).
    // Regardless of this setting, you can always explicitly call either of the following:
    // - b.sync.FunctionName()
    // - b.async_.FunctionName() (note the underscore to avoid a keyword conflict)
    default_client_mode async
}}
        "#, env!("CARGO_PKG_VERSION"))
            .trim_end()
        );
    }

    #[test]
    fn test_generate_content_ruby() {
        assert_eq!(
            generate_main_baml_content(&GeneratorOutputType::RubySorbet),
            format!(r#"
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {{
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet"
    output_type "ruby/sorbet"
    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"
    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The BAML VSCode extension version should also match this version.
    version "{}"
    // Valid values: "sync", "async"
    // This controls what `b.FunctionName()` will be (sync or async).
    // Regardless of this setting, you can always explicitly call either of the following:
    // - b.sync.FunctionName()
    // - b.async_.FunctionName() (note the underscore to avoid a keyword conflict)
    default_client_mode sync
}}
        "#, env!("CARGO_PKG_VERSION"))
            .trim_end()
        );
    }
}
