use std::{path::PathBuf, process::Command};

use anyhow::Result;
use baml_types::GeneratorOutputType;
use include_dir::include_dir;
use which::which;

#[derive(clap::Args, Debug)]
pub struct InitArgs {
    #[arg(
        long,
        help = "where to initialize the BAML project (default: current directory)",
        default_value = "."
    )]
    dest: PathBuf,

    #[arg(long, help = "Type of BAML client to generate.")]
    client_type: Option<GeneratorOutputType>,

    #[arg(
        long,
        help = r#"The OpenAPI client generator to run, if --client-type=rest/openapi.
Examples: "go", "java", "php", "ruby", "rust".  See full list at https://github.com/OpenAPITools/openapi-generator#overview."#
    )]
    openapi_client_type: Option<String>,
}

static SAMPLE_PROJECT: include_dir::Dir =
    include_dir!("$CARGO_MANIFEST_DIR/src/cli/initial_project");

/// TODO: one problem with this impl - this requires all users to install openapi-generator the same way
fn infer_openapi_command() -> Result<&'static str> {
    if which("openapi-generator").is_ok() {
        return Ok("openapi-generator");
    }

    if which("openapi-generator-cli").is_ok() {
        return Ok("openapi-generator-cli");
    }

    if which("npx").is_ok() {
        return Ok("npx @openapitools/openapi-generator-cli");
    }

    anyhow::bail!("Found none of openapi-generator, openapi-generator-cli, or npx in PATH")
}

impl InitArgs {
    pub fn run(&self, defaults: super::RuntimeCliDefaults) -> Result<()> {
        let output_type = self.client_type.unwrap_or(defaults.output_type);

        // If the destination directory already contains a baml_src directory, we don't want to overwrite it.
        let baml_src = self.dest.join("baml_src");
        if baml_src.exists() {
            anyhow::bail!(
                "Destination directory already contains a baml_src directory: {}",
                baml_src.display()
            );
        }

        SAMPLE_PROJECT.extract(&self.dest)?;
        // Also generate a main.baml file
        let main_baml = std::path::Path::new(&self.dest)
            .join("baml_src")
            .join("generators.baml");

        let openapi_generator_path = infer_openapi_command();

        if let Err(e) = &openapi_generator_path {
            log::warn!(
                "Failed to find openapi-generator-cli in your PATH, defaulting to using npx: {}",
                e
            );
        }

        let main_baml_content = generate_main_baml_content(
            output_type,
            openapi_generator_path.ok(),
            self.openapi_client_type.as_deref(),
        );
        std::fs::write(main_baml, main_baml_content)?;

        log::info!(
            "Created new BAML project in {} for {}",
            baml_src.display(),
            match output_type {
                GeneratorOutputType::PythonPydantic => "Python clients".to_string(),
                GeneratorOutputType::Typescript => "TypeScript clients".to_string(),
                GeneratorOutputType::RubySorbet => "Ruby clients".to_string(),
                GeneratorOutputType::OpenApi => match &self.openapi_client_type {
                    Some(s) => format!("{} clients via OpenAPI", s),
                    None => "REST clients".to_string(),
                },
            }
        );
        log::info!(
            "Follow instructions at https://docs.boundaryml.com/docs/get-started/quickstart/{}",
            match output_type {
                GeneratorOutputType::PythonPydantic => "python",
                GeneratorOutputType::Typescript => "typescript",
                GeneratorOutputType::RubySorbet => "ruby",
                GeneratorOutputType::OpenApi => "openapi",
            }
        );

        Ok(())
    }
}

fn generate_main_baml_content(
    output_type: GeneratorOutputType,
    openapi_generator_path: Option<&str>,
    openapi_client_type: Option<&str>,
) -> String {
    let default_client_mode = match output_type {
        GeneratorOutputType::OpenApi | GeneratorOutputType::RubySorbet => "".to_string(),
        GeneratorOutputType::PythonPydantic | GeneratorOutputType::Typescript => format!(
            r#"
    // Valid values: "sync", "async"
    // This controls what `b.FunctionName()` will be (sync or async).
    default_client_mode {}
    "#,
            output_type.recommended_default_client_mode()
        ),
    };
    let openapi_generate_command = if matches!(output_type, GeneratorOutputType::OpenApi) {
        let path = openapi_generator_path.unwrap_or("npx @openapitools/openapi-generator-cli");

        let cmd = format!(
            "{path} generate -i openapi.yaml -g {} -o .",
            openapi_client_type.unwrap_or("OPENAPI_CLIENT_TYPE"),
        );

        let openapi_generate_command = match openapi_client_type {
        Some("go") => format!(
            "{cmd} --additional-properties enumClassPrefix=true,isGoSubmodule=true,packageName=baml_client,withGoMod=false",
        ),
        Some("java") => format!(
            "{cmd} --additional-properties invokerPackage=com.boundaryml.baml_client,modelPackage=com.boundaryml.baml_client.model,apiPackage=com.boundaryml.baml_client.api,java8=true && mvn clean install",
        ),
        Some("php") => format!(
            "{cmd} --additional-properties composerPackageName=boundaryml/baml-client,invokerPackage=BamlClient",
        ),
        Some("ruby") => format!(
            "{cmd} --additional-properties gemName=baml_client",
        ),
        Some("rust") => format!(
            "{cmd} --additional-properties packageName=baml-client,avoidBoxedModels=true",
        ),
        _ => cmd,
    };

        let openapi_generate_command = match openapi_client_type {
            Some(_) => format!(
                r#"
    on_generate {:?}"#,
                openapi_generate_command
            ),
            None => format!(
                r#"
    //
    // Uncomment this line to tell BAML to automatically generate an OpenAPI client for you.
    //on_generate {:?}"#,
                openapi_generate_command
            ),
        };

        format!(
            r#"
    // 'baml-cli generate' will run this after generating openapi.yaml, to generate your OpenAPI client
    // This command will be run from within $output_dir/baml_client
    {}"#,
            openapi_generate_command.trim_start()
        )
    } else {
        "".to_string()
    };

    vec![
        format!(
        r#"
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {{
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet", "rest/openapi"
    output_type "{output_type}"

    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"

    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The BAML VSCode extension version should also match this version.
    version "{}""#,
            env!("CARGO_PKG_VERSION"),
        ),
        default_client_mode,
        openapi_generate_command,
    ]
    .iter()
    .filter_map(|s| if s.is_empty() { None } else { Some(s.as_str().trim_end()) })
    .chain(std::iter::once("}\n"))
    .collect::<Vec<_>>()
    .join("\n")
    .trim_start()
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_generate_content_pydantic() {
        assert_eq!(
            generate_main_baml_content(GeneratorOutputType::PythonPydantic, None, None),
            format!(
                r#"
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {{
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet", "rest/openapi"
    output_type "python/pydantic"

    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"

    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The BAML VSCode extension version should also match this version.
    version "{}"

    // Valid values: "sync", "async"
    // This controls what `b.FunctionName()` will be (sync or async).
    default_client_mode sync
}}
"#,
                env!("CARGO_PKG_VERSION")
            ).trim_start()
        );
    }

    #[test]
    fn test_generate_content_typescript() {
        assert_eq!(
            generate_main_baml_content(GeneratorOutputType::Typescript, None, None),
            format!(r#"
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {{
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet", "rest/openapi"
    output_type "typescript"

    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"

    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The BAML VSCode extension version should also match this version.
    version "{}"

    // Valid values: "sync", "async"
    // This controls what `b.FunctionName()` will be (sync or async).
    default_client_mode async
}}
"#,
                env!("CARGO_PKG_VERSION")
            ).trim_start()
        );
    }

    #[test]
    fn test_generate_content_ruby() {
        assert_eq!(
            generate_main_baml_content(GeneratorOutputType::RubySorbet, None, None),
            format!(r#"
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {{
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet", "rest/openapi"
    output_type "ruby/sorbet"

    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"

    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The BAML VSCode extension version should also match this version.
    version "{}"
}}
"#,
                env!("CARGO_PKG_VERSION")
            ).trim_start()
        );
    }

    #[test]
    fn test_generate_content_openapi_go() {
        assert_eq!(
            generate_main_baml_content(GeneratorOutputType::OpenApi, Some("openapi-generator"), Some("go")),
            format!(r#"
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {{
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet", "rest/openapi"
    output_type "rest/openapi"

    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"

    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The BAML VSCode extension version should also match this version.
    version "{}"

    // 'baml-cli generate' will run this after generating openapi.yaml, to generate your OpenAPI client
    // This command will be run from within $output_dir/baml_client
    on_generate "openapi-generator generate -i openapi.yaml -g go -o . --additional-properties enumClassPrefix=true,isGoSubmodule=true,packageName=baml_client,withGoMod=false"
}}
"#,
                env!("CARGO_PKG_VERSION")
            ).trim_start()
        );
    }

    #[test]
    fn test_generate_content_openapi_java() {
        assert_eq!(
            generate_main_baml_content(GeneratorOutputType::OpenApi, Some("openapi-generator"), Some("java")),
            format!(r#"
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {{
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet", "rest/openapi"
    output_type "rest/openapi"

    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"

    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The BAML VSCode extension version should also match this version.
    version "{}"

    // 'baml-cli generate' will run this after generating openapi.yaml, to generate your OpenAPI client
    // This command will be run from within $output_dir/baml_client
    on_generate "openapi-generator generate -i openapi.yaml -g java -o . --additional-properties invokerPackage=com.boundaryml.baml_client,modelPackage=com.boundaryml.baml_client.model,apiPackage=com.boundaryml.baml_client.api,java8=true && mvn clean install"
}}
"#,
                env!("CARGO_PKG_VERSION")
            ).trim_start()
        );
    }

    #[test]
    fn test_generate_content_openapi_unresolved_cli() {
        assert_eq!(
            generate_main_baml_content(GeneratorOutputType::OpenApi, None, None),
            format!(r#"
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {{
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet", "rest/openapi"
    output_type "rest/openapi"

    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"

    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The BAML VSCode extension version should also match this version.
    version "{}"

    // 'baml-cli generate' will run this after generating openapi.yaml, to generate your OpenAPI client
    // This command will be run from within $output_dir/baml_client
    //
    // Uncomment this line to tell BAML to automatically generate an OpenAPI client for you.
    //on_generate "npx @openapitools/openapi-generator-cli generate -i openapi.yaml -g OPENAPI_CLIENT_TYPE -o ."
}}
"#,
                env!("CARGO_PKG_VERSION")
            ).trim_start()
        );
    }
}
