use colored::Colorize;

use crate::{builder::get_src_dir, errors::CliError, shell::build_shell_command};

pub fn update_client(baml_dir: &Option<String>) -> Result<(), CliError> {
    let (_, (config, mut diagnostics)) = get_src_dir(baml_dir)?;
    diagnostics.to_result()?;

    let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();

    let errors: Vec<_> = config
        .generators
        .iter()
        .map(|(gen, _)| {
            // cd to the generator directory

            let cmd = shellwords::split(&gen.install_command)
                .map_err(|e| CliError::StringError(e.to_string()))?;

            let mut cmd = build_shell_command(cmd);

            println!(
                "Installing/Upgrading client for {}:\n  {} {}\n  {} {}",
                gen.language.as_str().green(),
                "project_root:".dimmed(),
                gen.project_root.to_string_lossy().yellow(),
                "install_command:".dimmed(),
                format!("{:?}", cmd).yellow()
            );

            // Get the project_root relative to cwd.
            let project_root = cwd.join(&gen.project_root);
            let project_root = project_root.canonicalize().map_err(|e| {
                CliError::StringError(format!(
                    "{}\nDirectory error: {}:\n{}",
                    "Failed!".red(),
                    gen.project_root.to_string_lossy(),
                    e
                ))
            })?;

            cmd.current_dir(&project_root);

            cmd.output()
                .map_err(|e| CliError::StringError(e.to_string()))
                .map_err(|e| CliError::StringError(format!("{} Error: {}", "Failed!".red(), e)))
                .and_then(|e| {
                    if !e.status.success() {
                        Err(CliError::StringError(format!(
                            "{}{}{}",
                            "Failed to add/update 'baml' python dependency!"
                                .normal()
                                .red(),
                            match String::from_utf8_lossy(&e.stdout) {
                                s if s.is_empty() => "".into(),
                                s => format!("\n{}", s.trim()),
                            },
                            match String::from_utf8_lossy(&e.stderr) {
                                s if s.is_empty() => "".into(),
                                s => format!("\n{}", s.trim()),
                            }
                        )))
                    } else {
                        println!(
                            "  {}\n{}",
                            "Success!".green(),
                            String::from_utf8_lossy(&e.stdout)
                                .lines()
                                .map(|l| format!("  {}", l))
                                .collect::<Vec<String>>()
                                .join("\n")
                                .to_string()
                                .dimmed()
                        );
                        Ok(())
                    }
                })
        })
        .filter_map(|e| e.err())
        .collect();

    if !errors.is_empty() {
        return Err(CliError::StringError(errors[0].to_string()));
    }

    Ok(())
}
