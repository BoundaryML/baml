use std::{env, fs, io::Write, path::PathBuf, process::Command};

use anyhow::Result;
use baml_types::GeneratorOutputType;
use include_dir::include_dir;
use which::which;

const BAML_EXTENSION_ID: &str = "boundary.baml-extension";

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

#[derive(Debug)]
enum EditorType {
    VSCode,
    Cursor,
    Unknown,
}

fn detect_editor() -> EditorType {
    // Check for Cursor first since it might also set VSCode-like variables
    if let Ok(cursor_trace_id) = env::var("CURSOR_TRACE_ID") {
        if !cursor_trace_id.is_empty() {
            return EditorType::Cursor;
        }
    }

    // Then check TERM_PROGRAM for both VSCode and Cursor
    if let Ok(term_program) = env::var("TERM_PROGRAM") {
        let term_lower = term_program.to_lowercase();
        if term_lower.contains("cursor") {
            return EditorType::Cursor;
        }
        if term_lower == "vscode" {
            return EditorType::VSCode;
        }
    }

    EditorType::Unknown
}

fn detect_and_install_extension() {
    let editor = detect_editor();

    match editor {
        EditorType::VSCode => {
            baml_log::info!("Detected VSCode terminal environment");
            install_vscode_extension();
        }
        EditorType::Cursor => {
            baml_log::info!("Detected Cursor terminal environment");
            install_cursor_extension();
        }
        EditorType::Unknown => {
            // Don't log anything for unknown editors to avoid noise
        }
    }
}

fn is_extension_installed(editor: &str) -> bool {
    let result = match editor {
        "code" => Command::new("code").args(&["--list-extensions"]).output(),
        "cursor" => Command::new("cursor").args(&["--list-extensions"]).output(),
        _ => return false,
    };

    match result {
        Ok(output) => {
            if output.status.success() {
                let extensions = String::from_utf8_lossy(&output.stdout);
                extensions
                    .lines()
                    .any(|line| line.trim() == BAML_EXTENSION_ID)
            } else {
                false
            }
        }
        Err(_) => false,
    }
}

fn install_vscode_extension() {
    // Check if 'code' command is available
    if which("code").is_ok() {
        // First check if extension is already installed
        if is_extension_installed("code") {
            baml_log::info!("BAML VSCode extension is already installed");
            return;
        }

        baml_log::info!("Installing BAML VSCode extension...");

        match Command::new("code")
            .args(&["--install-extension", BAML_EXTENSION_ID])
            .output()
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    baml_log::info!("Successfully installed BAML VSCode extension!");
                } else {
                    if stderr.contains("already installed") || stdout.contains("already installed")
                    {
                        baml_log::info!("BAML VSCode extension is already installed");
                    } else {
                        baml_log::warn!("Failed to install BAML VSCode extension");
                        baml_log::warn!("Exit code: {}", output.status.code().unwrap_or(-1));
                        baml_log::warn!("Stderr: {}", stderr);
                        baml_log::warn!("Stdout: {}", stdout);
                        baml_log::info!("Attempting manual installation...");
                        install_extension_manually("code");
                    }
                }
            }
            Err(e) => {
                baml_log::warn!("Failed to run VSCode command: {}", e);
                baml_log::info!("Attempting manual installation...");
                install_extension_manually("code");
            }
        }
    } else {
        baml_log::info!("VSCode 'code' command not found in PATH. Attempting to download and install extension manually...");
        install_extension_manually("code");
    }
}

fn install_cursor_extension() {
    // Check if 'cursor' command is available
    if which("cursor").is_ok() {
        // First check if extension is already installed
        if is_extension_installed("cursor") {
            baml_log::info!("BAML Cursor extension is already installed");
            return;
        }

        baml_log::info!("Installing BAML Cursor extension...");

        match Command::new("cursor")
            .args(&["--install-extension", BAML_EXTENSION_ID])
            .output()
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    baml_log::info!("Successfully installed BAML Cursor extension!");
                } else {
                    if stderr.contains("already installed") || stdout.contains("already installed")
                    {
                        baml_log::info!("BAML Cursor extension is already installed");
                    } else {
                        baml_log::warn!("Failed to install BAML Cursor extension");
                        baml_log::warn!("Exit code: {}", output.status.code().unwrap_or(-1));
                        baml_log::warn!("Stderr: {}", stderr);
                        baml_log::warn!("Stdout: {}", stdout);
                        baml_log::info!("Attempting manual installation...");
                        install_extension_manually("cursor");
                    }
                }
            }
            Err(e) => {
                baml_log::warn!("Failed to run Cursor command: {}", e);
                baml_log::info!("Attempting manual installation...");
                install_extension_manually("cursor");
            }
        }
    } else {
        baml_log::info!("Cursor command not found in PATH. Attempting to download and install extension manually...");
        install_extension_manually("cursor");
    }
}

fn download_vsix(url: &str, filename: &str) -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir();
    let vsix_path = temp_dir.join(filename);

    baml_log::info!("Downloading BAML extension to: {}", vsix_path.display());

    // Use curl to download the file
    let curl_args = vec!["-L", "-o", vsix_path.to_str().unwrap(), url];

    let output = Command::new("curl").args(&curl_args).output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to download extension: {}", stderr);
    }

    // Verify the file was downloaded
    if !vsix_path.exists() {
        anyhow::bail!("VSIX file was not created at expected location");
    }

    Ok(vsix_path)
}

fn install_extension_manually(editor: &str) {
    // Try to download the VSIX file
    let vsix_url = "https://marketplace.visualstudio.com/_apis/public/gallery/publishers/Boundary/vsextensions/baml-extension/latest/vspackage";
    let vsix_filename = "baml-extension.vsix";

    match download_vsix(vsix_url, vsix_filename) {
        Ok(vsix_path) => {
            baml_log::info!("Downloaded BAML extension to {}", vsix_path.display());

            // Try to find the editor executable in common locations
            let editor_paths = match editor {
                "code" => vec![
                    "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"
                        .to_string(),
                    "/usr/local/bin/code".to_string(),
                    "/opt/homebrew/bin/code".to_string(),
                    "C:\\Program Files\\Microsoft VS Code\\bin\\code.cmd".to_string(),
                    "C:\\Program Files (x86)\\Microsoft VS Code\\bin\\code.cmd".to_string(),
                ],
                "cursor" => {
                    let mut paths = vec![
                        "/Applications/Cursor.app/Contents/Resources/app/bin/cursor".to_string(),
                        "/usr/local/bin/cursor".to_string(),
                        "/opt/homebrew/bin/cursor".to_string(),
                        "C:\\Program Files\\Cursor\\cursor.exe".to_string(),
                    ];
                    if let Ok(username) = env::var("USERNAME") {
                        if !username.is_empty()
                            && username
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                        {
                            let user_path = format!(
                                "C:\\Users\\{}\\AppData\\Local\\Programs\\cursor\\cursor.exe",
                                username
                            );
                            paths.push(user_path);
                        }
                    }
                    paths
                }
                _ => vec![],
            };

            let mut installed = false;
            for path in &editor_paths {
                if std::path::Path::new(path).exists() {
                    baml_log::info!(
                        "Found {} at {}, attempting to install extension...",
                        editor,
                        path
                    );
                    if let Some(vsix_path_str) = vsix_path.to_str() {
                        match Command::new(path)
                            .args(&["--install-extension", vsix_path_str])
                            .output()
                        {
                            Ok(output) => {
                                if output.status.success() {
                                    baml_log::info!("Successfully installed BAML extension!");
                                    installed = true;
                                    break;
                                }
                            }
                            Err(_) => continue,
                        }
                    }
                }
            }

            if !installed {
                baml_log::info!(
                    "Could not automatically install the extension. Please install it manually:\n\
                    1. Open {} \n\
                    2. Go to Extensions (Cmd/Ctrl+Shift+X)\n\
                    3. Click the '...' menu and select 'Install from VSIX...'\n\
                    4. Select the downloaded file: {}",
                    if editor == "code" { "VSCode" } else { "Cursor" },
                    vsix_path.display()
                );
            }

            // Clean up the downloaded file after a delay to give user time to install manually
            let vsix_path_clone = vsix_path.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(300)); // 5 minutes
                let _ = fs::remove_file(vsix_path_clone);
            });
        }
        Err(e) => {
            baml_log::warn!("Failed to download extension: {}", e);
            baml_log::info!(
                "Please install the BAML extension manually from the {} marketplace",
                if editor == "code" { "VSCode" } else { "Cursor" }
            );
        }
    }
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
            baml_log::warn!(
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

        baml_log::info!(
            "Created new BAML project in {} for {}",
            baml_src.display(),
            match output_type {
                GeneratorOutputType::PythonPydanticV1 | GeneratorOutputType::PythonPydantic =>
                    "Python clients".to_string(),
                GeneratorOutputType::Typescript => "TypeScript clients".to_string(),
                GeneratorOutputType::RubySorbet => "Ruby clients".to_string(),
                GeneratorOutputType::OpenApi => match &self.openapi_client_type {
                    Some(s) => format!("{s} clients via OpenAPI"),
                    None => "REST clients".to_string(),
                },
                GeneratorOutputType::TypescriptReact => "TypeScript React clients".to_string(),
                GeneratorOutputType::Go => "Go clients".to_string(),
            }
        );
        baml_log::info!(
            "Follow instructions at https://docs.boundaryml.com/docs/get-started/quickstart/{}",
            match output_type {
                GeneratorOutputType::PythonPydanticV1 | GeneratorOutputType::PythonPydantic =>
                    "python",
                GeneratorOutputType::Typescript => "typescript",
                GeneratorOutputType::RubySorbet => "ruby",
                GeneratorOutputType::OpenApi => "openapi",
                GeneratorOutputType::TypescriptReact => "typescript-react",
                GeneratorOutputType::Go => "go",
            }
        );

        // Detect and install VSCode/Cursor extension
        detect_and_install_extension();

        Ok(())
    }
}

fn generate_main_baml_content(
    output_type: GeneratorOutputType,
    openapi_generator_path: Option<&str>,
    openapi_client_type: Option<&str>,
) -> String {
    let default_client_mode = match output_type {
        GeneratorOutputType::OpenApi
        | GeneratorOutputType::RubySorbet
        | GeneratorOutputType::Go => "".to_string(),
        GeneratorOutputType::PythonPydantic
        | GeneratorOutputType::PythonPydanticV1
        | GeneratorOutputType::Typescript
        | GeneratorOutputType::TypescriptReact => format!(
            r#"
    // Valid values: "sync", "async"
    // This controls what `b.FunctionName()` will be (sync or async).
    default_client_mode {}
    "#,
            output_type.recommended_default_client_mode()
        ),
    };
    let generate_command = if matches!(output_type, GeneratorOutputType::OpenApi) {
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
    on_generate {openapi_generate_command:?}"#
            ),
            None => format!(
                r#"
    //
    // Uncomment this line to tell BAML to automatically generate an OpenAPI client for you.
    //on_generate {openapi_generate_command:?}"#
            ),
        };

        format!(
            r#"
    // 'baml-cli generate' will run this after generating openapi.yaml, to generate your OpenAPI client
    // This command will be run from within $output_dir/baml_client
    {}"#,
            openapi_generate_command.trim_start()
        )
    } else if matches!(output_type, GeneratorOutputType::Go) {
        String::from(
            r#"
    // 'baml-cli generate' will run this after generating go code
    // This command will be run from within $output_dir/baml_client
    on_generate "gofmt -w . && goimports -w ."
    "#,
        )
    } else {
        "".to_string()
    };
    let go_client_package_name = if matches!(output_type, GeneratorOutputType::Go) {
        // Find the package name in the go.mod file
        if let Ok(go_mod) = std::fs::read_to_string("go.mod") {
            if let Some(package_name) = go_mod.lines().find_map(|line| line.strip_prefix("module "))
            {
                Some(package_name.to_string())
            } else {
                Some("YOUR_PACKAGE_NAME".to_string())
            }
        } else {
            Some("YOUR_PACKAGE_NAME".to_string())
        }
    } else {
        None
    };

    let go_client_package_name = match go_client_package_name {
        Some(package_name) => {
            if package_name == "YOUR_PACKAGE_NAME" {
                baml_log::warn!("Failed to find go.mod file, please update the client_package_name in your generators.baml file");
            }
            format!(
                r#"
    // Your Go packages name as specified in go.mod
    // We need this to generate correct imports in the generated baml_client
    client_package_name "{package_name}""#
            )
        }
        None => "".to_string(),
    };

    [
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
        generate_command,
        go_client_package_name,
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
    use std::env;

    use pretty_assertions::assert_eq;

    use super::*;

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
