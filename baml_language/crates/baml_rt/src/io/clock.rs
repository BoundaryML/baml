//! Clock sysops only carry owned data across an await.
use super::{Failure, Output};
use num_bigint::BigInt;
use serde_json::Value;
fn failure(error: bex_vm_types::errors::VmRustFnError) -> Failure {
    match error {
        bex_vm_types::errors::VmRustFnError::BamlError(bex_vm_types::errors::VmBamlError::Io {
            message,
        }) => Failure::io(message),
        error => Failure {
            class: "InternalError",
            message: error.to_string(),
            resource: String::new(),
        },
    }
}
pub fn now() -> Output {
    Output::Ready(Ok(Value::String(bex_vm_types::clock::now().to_string())))
}
pub fn system_timezone() -> Output {
    Output::Ready(
        bex_vm_types::clock::system_timezone()
            .map(Value::String)
            .map_err(failure),
    )
}
pub fn offset_at(timezone: String, at: BigInt) -> Output {
    Output::Ready(Ok(
        bex_vm_types::clock::offset_at(&timezone, &at).map_or(Value::Null, Value::from)
    ))
}
pub fn to_instant(timezone: String, civil: BigInt, disambiguation: String) -> Output {
    Output::Ready(
        bex_vm_types::clock::to_instant(&timezone, &civil, &disambiguation)
            .map(|n| n.map_or(Value::Null, |n| Value::String(n.to_string())))
            .map_err(failure),
    )
}
pub fn sleep(nanos: BigInt) -> Output {
    let duration = bex_vm_types::clock::timeout_from_nanos(&nanos).unwrap_or_default();
    Output::Async(Box::pin(async move {
        tokio::time::sleep(duration).await;
        Ok(Value::Null)
    }))
}
pub fn host_info() -> Output {
    use std::io::IsTerminal;
    Output::Ready(Ok(
        serde_json::json!({"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"cpu_count":std::thread::available_parallelism().ok().and_then(|n|i64::try_from(n.get()).ok()).unwrap_or(0),"stderr_is_terminal":std::io::stderr().is_terminal()}),
    ))
}
