use std::ffi::OsStr;
use std::process::Command;

use log::info;

use crate::errors::CliError;

fn run_command<I, S>(program: &str, args: I, error_prefix: &'static str) -> Result<String, CliError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr> + std::fmt::Debug,
{
    let mut cmd = Command::new(program);
    cmd.args(args);

    info!(
        "Running: {} {:?}",
        program,
        cmd.get_args().collect::<Vec<_>>()
    );
    let response = match cmd.output() {
        Ok(response) => response,
        Err(error) => {
            return Err(("Shell command failed", error).into());
        }
    };
    match response.status.success() {
        true => Ok(String::from_utf8_lossy(&response.stdout).into_owned()),
        false => Err(format!(
            "{}\n{}",
            error_prefix,
            String::from_utf8_lossy(&response.stderr)
        )
        .into()),
    }
}

pub fn run_command_with_error<I, S>(
    program: &str,
    args: I,
    error_prefix: &'static str,
) -> Result<(), CliError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr> + std::fmt::Debug,
{
    run_command(program, args, error_prefix)?;
    Ok(())
}
