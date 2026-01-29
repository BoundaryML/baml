//! System operations.

use std::sync::Arc;

use bex_heap::BexHeap;
use sys_types::{BexExternalValue, BexValue, OpError, SysOpResult};
use tokio::process::Command;

/// Execute a shell command and return stdout.
///
/// Signature: `fn shell(command: String) -> String`
pub(crate) fn shell(_heap: Arc<BexHeap>, args: &[BexValue]) -> SysOpResult {
    let command = match extract_string(args.first()) {
        Ok(c) => c,
        Err(e) => return SysOpResult::Ready(Err(e)),
    };
    SysOpResult::Async(Box::pin(shell_async(command)))
}

fn extract_string(value: Option<&BexValue>) -> Result<String, OpError> {
    match value {
        Some(BexValue::External(BexExternalValue::String(s))) => Ok(s.clone()),
        other => Err(OpError::TypeError {
            expected: "string command",
            actual: format!("{other:?}"),
        }),
    }
}

async fn shell_async(command: String) -> Result<BexExternalValue, OpError> {
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
