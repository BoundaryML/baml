use internal_baml_core::configuration::GeneratorOutputType;
use semver::Version;

#[derive(Debug, PartialEq)]
pub struct VersionCheckError {
    msg: String,
}

impl VersionCheckError {
    pub fn msg(&self) -> String {
        format!("Version mismatch: {}", self.msg)
    }
}

#[derive(Debug, PartialEq)]
pub enum GeneratorType {
    VSCodeCLI,
    VSCode,
    CLI,
}

#[derive(Debug, PartialEq)]
pub enum VersionCheckMode {
    Strict,
    None,
}

pub fn check_version(
    generator_version: &str,
    current_version: &str,
    generator_type: GeneratorType,
    version_check_mode: VersionCheckMode,
    generator_language: GeneratorOutputType,
    is_diagnostic: bool,
) -> Option<VersionCheckError> {
    if version_check_mode == VersionCheckMode::None {
        return None;
    }

    let gen_version = match Version::parse(generator_version) {
        Ok(v) => v,
        Err(_) => {
            return Some(VersionCheckError {
                msg: format!("Invalid generator version in BAML config: {generator_version}"),
            })
        }
    };

    let runtime_version = match Version::parse(current_version) {
        Ok(v) => v,
        Err(_) => {
            return Some(VersionCheckError {
                msg: format!("Invalid current version: {current_version}"),
            })
        }
    };

    if generator_version == "0.0.0" {
        let error_message = format!("A 'version' is now required in generator config. Please add 'version \"{current_version}\"' inside the generator to continue generating baml_client.\n\nMake sure your installed baml package dependency and VSCode are also version {current_version} \n\nSee https://docs.boundaryml.com/docs/calling-baml/generate-baml-client");
        return Some(VersionCheckError {
            msg: if !is_diagnostic {
                format!("⚠️⚠️⚠️ BAML GENERATION DISABLED: {error_message}")
            } else {
                error_message
            },
        });
    }

    if gen_version.major != runtime_version.major || gen_version.minor != runtime_version.minor {
        let base_message = format!(
            "Generator version ({}) does not match the {} version ({}). Major and minor versions must match.",
            gen_version,
            match generator_type {
                GeneratorType::VSCode => "VSCode extension",
                GeneratorType::CLI | GeneratorType::VSCodeCLI => "installed baml package",
            },
            runtime_version
        );

        let (update_message, docs_link) = if runtime_version > gen_version {
            (
                match generator_type {
                    GeneratorType::VSCode => 
                        format!("Update the 'version' in your BAML generator config to '{runtime_version}' to match the VSCode extension version."),
                    GeneratorType::CLI | GeneratorType::VSCodeCLI => 
                        format!("Update the 'version' in your BAML generator config to '{runtime_version}' to match the installed baml package version."),
                },
                "https://docs.boundaryml.com/docs/calling-baml/generate-baml-client#troubleshooting-version-conflicts"
            )
        } else {
            if matches!(generator_language, GeneratorOutputType::OpenApi) {
                (
                    match generator_type {
                        GeneratorType::VSCode => {
                                format!("Update your VSCode extension to version '{gen_version}' to match the version in the BAML generator config, and use 'npx @boundaryml/baml@{gen_version}' to run the CLI.")
                        }
                        GeneratorType::VSCodeCLI | GeneratorType::CLI => {
                            format!("Use BAML v{gen_version} to match the version in the BAML generator config, like so: npx @boundaryml/baml@{gen_version} generate")
                        },
                    },
                    "https://docs.boundaryml.com/docs/calling-baml/generate-baml-client#troubleshooting-version-conflicts"
                )
            } else {
                let update_instruction = match generator_language {
                    GeneratorOutputType::OpenApi => {
                        format!("use 'npx @boundaryml/baml@{gen_version}'")
                    }
                    GeneratorOutputType::PythonPydantic => {
                        format!("pip install --upgrade baml-py=={gen_version}")
                    }
                    GeneratorOutputType::Typescript => {
                        format!("npm install --save-dev @boundaryml/baml@{gen_version}")
                    }
                    GeneratorOutputType::RubySorbet => format!("gem install baml -v {gen_version}"),
                };
                (
                    match generator_type {
                        GeneratorType::VSCode => {
                                format!("Update your VSCode extension to version '{gen_version}' to match the version in the BAML generator config. Also update your BAML package: {update_instruction}")
                        }
                        GeneratorType::VSCodeCLI | GeneratorType::CLI => {
                            format!("Update your installed BAML CLI package to version '{gen_version}' to match the version in the BAML generator config: {update_instruction}")
                        },
                    },
                    "https://docs.boundaryml.com/docs/calling-baml/generate-baml-client#troubleshooting-version-conflicts"
                )
            }
        };

        let formatted_link = match is_diagnostic {
            false => format!("[documentation]({docs_link})"),
            _ => docs_link.to_string(),
        };

        let error_message = format!(
            "{base_message}\n\nAction required: {update_message}\n\nTo prevent this issue, see: {formatted_link}"
        );

        return Some(VersionCheckError {
            msg: if !is_diagnostic {
                format!("⚠️⚠️⚠️ BAML GENERATION DISABLED: {error_message}")
            } else {
                error_message
            },
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_check_none() {
        assert_eq!(
            check_version(
                "1.0.0",
                "2.0.0",
                GeneratorType::CLI,
                VersionCheckMode::None,
                GeneratorOutputType::PythonPydantic,
                false
            ),
            None
        );
    }

    #[test]
    fn test_invalid_generator_version() {
        let result = check_version(
            "invalid",
            "1.0.0",
            GeneratorType::CLI,
            VersionCheckMode::Strict,
            GeneratorOutputType::PythonPydantic,
            false,
        );
        assert!(result.is_some());
        assert!(result.unwrap().msg.contains("Invalid generator version"));
    }

    #[test]
    fn test_invalid_current_version() {
        let result = check_version(
            "1.0.0",
            "invalid",
            GeneratorType::CLI,
            VersionCheckMode::Strict,
            GeneratorOutputType::PythonPydantic,
            false,
        );
        assert!(result.is_some());
        assert!(result.unwrap().msg.contains("Invalid current version"));
    }

    #[test]
    fn test_matching_versions() {
        assert_eq!(
            check_version(
                "1.2.3",
                "1.2.4",
                GeneratorType::CLI,
                VersionCheckMode::Strict,
                GeneratorOutputType::PythonPydantic,
                false
            ),
            None
        );
    }

    #[test]
    fn test_mismatched_major_version_cli_python() {
        let result = check_version(
            "2.0.0",
            "1.0.0",
            GeneratorType::CLI,
            VersionCheckMode::Strict,
            GeneratorOutputType::PythonPydantic,
            false,
        );
        assert!(result.is_some());
        let error_msg = result.unwrap().msg();
        assert!(error_msg.contains("Version mismatch"));
        assert!(error_msg.contains("installed BAML CLI"));
        assert!(error_msg.contains("pip install --upgrade baml-py==2.0.0"));
    }

    #[test]
    fn test_mismatched_minor_version_vscode_typescript() {
        let result = check_version(
            "1.3.0",
            "1.2.0",
            GeneratorType::VSCode,
            VersionCheckMode::Strict,
            GeneratorOutputType::Typescript,
            false,
        );
        assert!(result.is_some());
        let error_msg = result.unwrap().msg();
        println!("{error_msg}");
        assert!(error_msg.contains("Version mismatch"));
        assert!(error_msg.contains("VSCode extension"));
        assert!(error_msg.contains("npm install --save-dev @boundaryml/baml@1.3.0"));
    }

    #[test]
    fn test_older_vscode_version_ruby() {
        let result = check_version(
            "1.3.0",
            "1.2.0",
            GeneratorType::VSCodeCLI,
            VersionCheckMode::Strict,
            GeneratorOutputType::RubySorbet,
            false,
        );
        assert!(result.is_some());
        let error_msg = result.unwrap().msg();
        assert!(error_msg.contains("Version mismatch"));
        assert!(error_msg.contains("baml package"));
        assert!(error_msg.contains("gem install baml -v 1.3.0"));
    }

    #[test]
    fn test_patch_version_difference() {
        assert_eq!(
            check_version(
                "1.2.3",
                "1.2.4",
                GeneratorType::CLI,
                VersionCheckMode::Strict,
                GeneratorOutputType::PythonPydantic,
                false
            ),
            None
        );
    }
}
