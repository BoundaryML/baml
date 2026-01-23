//! System operations.

use tokio::process::Command;

use crate::{BexExternalValue, OpError};

/// Execute a shell command and return stdout.
///
/// Signature: `fn shell(command: String) -> String`
pub async fn shell(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let command = match args.into_iter().next() {
        Some(BexExternalValue::String(s)) => s,
        other => {
            return Err(OpError::TypeError {
                expected: "string command",
                actual: format!("{other:?}"),
            });
        }
    };

    let output = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| OpError::Other(format!("Failed to execute command '{command}': {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        return Err(OpError::Other(format!(
            "Command '{}' failed with exit code {}: {}",
            command,
            code,
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(BexExternalValue::String(stdout))
}
