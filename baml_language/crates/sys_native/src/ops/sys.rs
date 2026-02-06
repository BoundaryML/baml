//! System operations.

use std::sync::Arc;

use bex_heap::BexHeap;
use sys_types::{BexExternalValue, OpError, OpErrorKind, SysOp, SysOpResult};
use tokio::process::Command;

/// Execute a shell command and return stdout.
///
/// Signature: `fn shell(command: String) -> String`
pub(crate) fn shell(heap: &Arc<BexHeap>, mut args: Vec<bex_heap::BexValue<'_>>) -> SysOpResult {
    let err = |kind: OpErrorKind| SysOpResult::Ready(Err(OpError::new(SysOp::Shell, kind)));

    if args.len() != 1 {
        return err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.remove(0);
    let command =
        match heap.with_gc_protection(move |protected| arg0.as_string(&protected).cloned()) {
            Ok(command) => command,
            Err(e) => return err(e.into()),
        };

    SysOpResult::Async(Box::pin(async move {
        shell_async(command)
            .await
            .map_err(|e| OpError::new(SysOp::Shell, e))
    }))
}

async fn shell_async(command: String) -> Result<BexExternalValue, OpErrorKind> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to execute command '{command}': {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        return Err(OpErrorKind::Other(format!(
            "Command '{}' failed with exit code {}: {}",
            command,
            code,
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(BexExternalValue::String(stdout))
}
