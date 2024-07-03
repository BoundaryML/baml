use std::process::Command;

// Function for safely running a shell command (including chained commands)
#[allow(dead_code)]
pub fn build_shell_command(cmd: Vec<String>) -> Command {
    // Check if the command is a chained command (i.e. contains a pipe or a semicolon or a double ampersand)
    let is_chained = cmd
        .iter()
        .any(|s| s.contains(|c| c == '|' || c == ';' || c == '&'));

    if is_chained {
        build_chained_shell_command(cmd)
    } else {
        build_single_shell_command(cmd)
    }
}

#[allow(dead_code)]
fn build_chained_shell_command(cmd: Vec<String>) -> Command {
    let shell_command = cmd
        .iter()
        .map(|s| {
            if s == "&&" {
                s.clone()
            } else {
                shellwords::escape(s)
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let cmd = if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C");
        cmd.arg(shell_command);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.arg("-c");
        cmd.arg(shell_command);
        cmd
    };

    cmd
}

#[allow(dead_code)]
fn build_single_shell_command(cmd: Vec<String>) -> Command {
    let mut cmd_ = Command::new(cmd[0].clone());
    cmd_.args(&cmd[1..]);

    cmd_
}
