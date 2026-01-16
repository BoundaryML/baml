//! System operations.
//!
//! Implements `baml.sys.shell`.

use std::sync::Arc;

use tokio::process::Command;

use crate::{OpContext, OpError, ResolvedArgs, ResolvedValue};

// ============================================================================
// baml.sys.shell
// ============================================================================

/// Execute a shell command and return stdout.
///
/// Signature: `fn shell(command: String) -> String`
pub async fn shell(_ctx: Arc<OpContext>, args: ResolvedArgs) -> Result<ResolvedValue, OpError> {
    // Extract the command argument
    let command = match args.args.into_iter().next() {
        Some(ResolvedValue::String(s)) => s,
        other => {
            let msg = format!("Expected string command argument, got: {other:?}");
            return Err(OpError::Other(msg));
        }
    };

    // Execute the command using sh -c
    let output = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| OpError::Other(format!("Failed to execute command '{command}': {e}")))?;

    // Check for non-zero exit status
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

    // Return stdout as string
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(ResolvedValue::String(stdout))
}
