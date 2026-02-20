use async_trait::async_trait;
use bex_engine::BexEngine;
use bex_heap::{BexExternalValue, BexValue};
use sys_types::{CallId, CancellationToken};

use crate::{BexArgs, RuntimeError, project::BexProject};

/// Core runtime API: call functions and introspect parameters.
#[async_trait]
pub trait Bex: Send + Sync {
    /// Execute a function by name. Returns a fully owned value (no Handle variants).
    async fn call_function(
        &self,
        function_name: &str,
        args: BexArgs,
        call_id: CallId,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, RuntimeError>;
}

#[async_trait]
impl Bex for BexProject {
    async fn call_function(
        &self,
        function_name: &str,
        args: BexArgs,
        call_id: CallId,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, RuntimeError> {
        let bex = self.get_bex()?;
        Bex::call_function(&*bex, function_name, args, call_id, cancel).await
    }
}

/// Resolve named `BexArgs` into the positional `Vec<BexExternalValue>` that
/// `BexEngine::call_function` expects, using the engine's parameter metadata.
async fn call_engine(
    engine: &BexEngine,
    function_name: &str,
    BexArgs(mut args): BexArgs,
    call_id: CallId,
    cancel: CancellationToken,
) -> Result<BexExternalValue, RuntimeError> {
    let params = engine
        .function_params(function_name)
        .map_err(RuntimeError::from)?;

    let ordered_args: Vec<BexExternalValue> = params
        .into_iter()
        .map(|(name, _)| {
            args.remove(name)
                .ok_or_else(|| RuntimeError::InvalidArgument {
                    name: name.to_string(),
                })
        })
        .collect::<Result<_, _>>()?;

    if !args.is_empty() {
        let extra_args = args.keys().cloned().collect::<Vec<_>>().join(", ");
        return Err(RuntimeError::InvalidArgument {
            name: format!("extra arguments: {extra_args}"),
        });
    }

    let result = BexEngine::call_function(
        engine,
        function_name,
        ordered_args,
        call_id,
        None,
        &[],
        cancel,
    )
    .await?;

    let owned_result = engine
        .heap()
        .with_gc_protection(|p| BexValue::from(&result).as_owned_but_very_slow(&p))?;

    Ok(owned_result)
}

#[async_trait]
impl Bex for BexEngine {
    async fn call_function(
        &self,
        function_name: &str,
        args: BexArgs,
        call_id: CallId,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, RuntimeError> {
        call_engine(self, function_name, args, call_id, cancel).await
    }
}

#[async_trait]
impl Bex for std::sync::Arc<BexEngine> {
    async fn call_function(
        &self,
        function_name: &str,
        args: BexArgs,
        call_id: CallId,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, RuntimeError> {
        Bex::call_function(self.as_ref(), function_name, args, call_id, cancel).await
    }
}
