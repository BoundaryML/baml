use internal_baml_core::configuration::GeneratorOutputType;
use semver::Version;

#[derive(Debug, PartialEq)]
pub struct VersionCheckError {
    pub msg: String,
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
        Err(_) => return Some(VersionCheckError {
            msg: format!("Invalid generator version in BAML config: {}", generator_version),
        }),
    };
  
    let runtime_version = match Version::parse(current_version) {
        Ok(v) => v,
        Err(_) => return Some(VersionCheckError {
            msg: format!("Invalid current version: {}", current_version),
        }),
    };
  
    if generator_version == "0.0.0" {
        let error_message = format!("A 'version' is now required in generator config. Please add 'version \"{}\"' inside the generator to continue generating baml_client.\n\nMake sure your installed baml package dependency and VSCode are also version {} \n\nSee https://docs.boundaryml.com/docs/calling-baml/generate-baml-client", current_version, current_version);
        return Some(VersionCheckError {
            msg: if !is_diagnostic {
                format!("⚠️⚠️⚠️ BAML GENERATION DISABLED: {}", error_message)
            } else {
                error_message
            },
        });
    }
  
    if gen_version.major != runtime_version.major || gen_version.minor != runtime_version.minor {
        let base_message = format!(
            "Version mismatch: Generator version ({}) does not match the {} version ({}). Major and minor versions must match.",
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
                        format!("Update the 'version' in your BAML generator config to '{}' to match the VSCode extension version.", runtime_version),
                    GeneratorType::CLI | GeneratorType::VSCodeCLI => 
                        format!("Update the 'version' in your BAML generator config to '{}' to match the installed baml package version.", runtime_version),
                },
                "https://docs.boundaryml.com/docs/calling-baml/generate-baml-client#troubleshooting-version-conflicts"
            )
        } else {
            let update_instruction = match generator_language {
                GeneratorOutputType::PythonPydantic => format!("pip install --upgrade baml-py=={}", gen_version),
                GeneratorOutputType::Typescript => format!("npm install --save-dev @boundaryml/baml@{}", gen_version),
                GeneratorOutputType::RubySorbet => format!("gem install baml -v {}", gen_version),
            };
            (
                match generator_type {
                   GeneratorType::VSCode => 
                        format!("Update your VSCode extension to version '{}' to match the version in the BAML generator config. Also update your BAML package: {}", gen_version, update_instruction),
                        GeneratorType::VSCodeCLI | GeneratorType::CLI => 
                        format!("Update your installed BAML CLI package to version '{}' to match the version in the BAML generator config: {}", gen_version, update_instruction),
                },
                "https://docs.boundaryml.com/docs/calling-baml/generate-baml-client#troubleshooting-version-conflicts"
            )
        };
  
        let formatted_link = match is_diagnostic {
            false => format!("[documentation]({})", docs_link),
            _ => docs_link.to_string(),
        };
  
        let error_message = format!(
            "{}\n\nAction required: {}\n\nTo prevent this issue, see: {}",
            base_message, update_message, formatted_link
        );
  
        return Some(VersionCheckError { 
            msg: if !is_diagnostic {
                format!("⚠️⚠️⚠️ BAML GENERATION DISABLED: {}", error_message)
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
    fn test_vscode_link_formatting() {
        let result = check_version("1.3.0", "1.2.0", GeneratorType::VSCode, VersionCheckMode::Strict, GeneratorOutputType::Typescript, false);
        assert!(result.is_some());
        let error_msg = result.unwrap().msg;
        assert!(error_msg.contains("[documentation](https://docs.boundaryml.com/docs/calling-baml/generate-baml-client#updating-baml-package)"));
    }

    #[test]
    fn test_cli_link_formatting() {
        let result = check_version("1.3.0", "1.2.0", GeneratorType::CLI, VersionCheckMode::Strict, GeneratorOutputType::PythonPydantic, false);
        assert!(result.is_some());
        let error_msg = result.unwrap().msg;
        assert!(error_msg.contains("https://docs.boundaryml.com/docs/calling-baml/generate-baml-client#updating-baml-package"));
        assert!(!error_msg.contains("[documentation]"));
    }
    #[test]
    fn test_version_check_none() {
        assert_eq!(
            check_version("1.0.0", "2.0.0", GeneratorType::CLI, VersionCheckMode::None, GeneratorOutputType::PythonPydantic, false),
            None
        );
    }

    #[test]
    fn test_invalid_generator_version() {
        let result = check_version("invalid", "1.0.0", GeneratorType::CLI, VersionCheckMode::Strict, GeneratorOutputType::PythonPydantic, false);
        assert!(result.is_some());
        assert!(result.unwrap().msg.contains("Invalid generator version"));
    }

    #[test]
    fn test_invalid_current_version() {
        let result = check_version("1.0.0", "invalid", GeneratorType::CLI, VersionCheckMode::Strict, GeneratorOutputType::PythonPydantic, false);
        assert!(result.is_some());
        assert!(result.unwrap().msg.contains("Invalid current version"));
    }

    #[test]
    fn test_matching_versions() {
        assert_eq!(
            check_version("1.2.3", "1.2.4", GeneratorType::CLI, VersionCheckMode::Strict, GeneratorOutputType::PythonPydantic, false),
            None
        );
    }

    #[test]
    fn test_mismatched_major_version_cli_python() {
        let result = check_version("2.0.0", "1.0.0", GeneratorType::CLI, VersionCheckMode::Strict, GeneratorOutputType::PythonPydantic, false);
        assert!(result.is_some());
        let error_msg = result.unwrap().msg;
        assert!(error_msg.contains("Version mismatch"));
        assert!(error_msg.contains("installed BAML CLI"));
        assert!(error_msg.contains("pip install --upgrade baml==2.0.0"));
    }

    #[test]
    fn test_mismatched_minor_version_vscode_typescript() {
        let result = check_version("1.3.0", "1.2.0", GeneratorType::VSCode, VersionCheckMode::Strict, GeneratorOutputType::Typescript, false);
        assert!(result.is_some());
        let error_msg = result.unwrap().msg;
        assert!(error_msg.contains("Version mismatch"));
        assert!(error_msg.contains("VSCode extension"));
        assert!(error_msg.contains("npm install --save-dev @baml/baml@1.3.0"));
    }

    #[test]
    fn test_older_vscode_version_ruby() {
        let result = check_version("1.3.0", "1.2.0", GeneratorType::VSCodeCLI, VersionCheckMode::Strict, GeneratorOutputType::RubySorbet, false);
        assert!(result.is_some());
        let error_msg = result.unwrap().msg;
        assert!(error_msg.contains("Version mismatch"));
        assert!(error_msg.contains("VSCode extension"));
        assert!(error_msg.contains("gem install baml -v 1.3.0"));
    }

    #[test]
    fn test_patch_version_difference() {
        assert_eq!(
            check_version("1.2.3", "1.2.4", GeneratorType::CLI, VersionCheckMode::Strict, GeneratorOutputType::PythonPydantic, false),
            None
        );
    }
}
