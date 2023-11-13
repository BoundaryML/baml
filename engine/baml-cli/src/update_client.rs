use crate::{builder::get_src_dir, command::run_command_with_error, errors::CliError};

pub fn update_client(baml_dir: &Option<String>) -> Result<(), CliError> {
    let (_, (config, mut diagnostics)) = get_src_dir(baml_dir)?;
    diagnostics.to_result()?;

    config.generators.iter().for_each(|(gen, _)| {
        // cd to the generator directory
        std::env::set_current_dir(&gen.output).unwrap();
        let errs = match (gen.language.as_str(), gen.pkg_manager.as_deref()) {
            ("python", Some("poetry")) => run_command_with_error(
                "poetry",
                &["add", "baml@latest", "--no-cache"],
                "Failed to poetry update baml",
            ),
            ("python", Some("pip3")) => run_command_with_error(
                "pip3",
                &["install", "--upgrade", "--no-cache-dir", "baml"],
                "Failed to update client",
            ),
            ("python", Some("pip")) => run_command_with_error(
                "pip",
                &["install", "--upgrade", "--no-cache-dir", "baml"],
                "Failed to update client",
            ),
            ("python", Some(other)) => {
                run_command_with_error(other, &["baml"], "Failed to update client")
            }
            ("python", None) => Ok(()),
            _ => Ok(()),
        };
        if let Err(err) = errs {
            log::warn!("{}", err);
        }
    });

    Ok(())
}
