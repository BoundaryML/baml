//! System operations.

use bex_external_types::BexExternalValue;
use sys_types::{OpError, SysOpResult};
use tokio::process::Command;

/// Execute a shell command and return stdout.
///
/// Signature: `fn shell(command: String) -> String`
pub(crate) fn shell(args: Vec<BexExternalValue>) -> SysOpResult {
    SysOpResult::Async(Box::pin(shell_async(args)))
}

async fn shell_async(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
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
