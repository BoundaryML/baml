use std::ffi::OsStr;
use std::process::Command;

use log::info;

fn run_command<I, S>(program: &str, args: I) -> Result<String, (String, String)>
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
    let command_result = cmd.output();
    match command_result {
        Ok(response) => {
            if response.status.success() {
                Ok(String::from_utf8_lossy(&response.stdout).into_owned())
            } else {
                Err((
                    String::from_utf8_lossy(&response.stderr).to_string(),
                    String::from_utf8_lossy(&response.stdout).to_string(),
                ))
            }
        }
        Err(err) => Err((err.to_string(), "".to_string())),
    }
}

pub fn run_command_with_error<I, S>(
    program: &str,
    args: I,
    error_msg: &'static str,
) -> Result<(), (&'static str, Option<String>)>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr> + std::fmt::Debug,
{
    if let Err(error) = run_command(program, args) {
        return Err((error_msg, Some(error.0)));
    }
    Ok(())
}
