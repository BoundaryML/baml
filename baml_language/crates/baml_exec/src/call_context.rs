use bex_engine::{
    CancellationToken, FunctionCallContext, FunctionCallContextBuilder, RuntimeTy,
    logger::TraceLogger,
};
use indexmap::IndexMap;

/// Logger and cancellation settings inherited by helper calls.
///
/// JSON argument and output conversion run as distinct engine calls, so they
/// need fresh call contexts. Profiling admission is intentionally not inherited.
#[derive(Clone)]
pub struct HelperCallContext {
    logger: TraceLogger,
    cancel: CancellationToken,
}

impl HelperCallContext {
    #[must_use]
    pub fn from_call_context(context: &FunctionCallContext) -> Self {
        Self {
            logger: context.logger.clone(),
            cancel: context.cancel.clone(),
        }
    }

    #[must_use]
    pub fn disabled() -> Self {
        Self {
            logger: TraceLogger::disabled(),
            cancel: CancellationToken::new(),
        }
    }

    #[must_use]
    pub(crate) fn call_context(
        &self,
        type_args: IndexMap<String, RuntimeTy>,
    ) -> FunctionCallContext {
        FunctionCallContextBuilder::new(bex_engine::CallId::next())
            .with_logger(self.logger.clone())
            .with_cancel_token(self.cancel.clone())
            .with_type_args(type_args)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_context_inherits_cancellation() {
        let parent = FunctionCallContextBuilder::new(bex_engine::CallId::next()).build();
        let helper_context = HelperCallContext::from_call_context(&parent);
        let helper = helper_context.call_context(IndexMap::new());

        parent.cancel.cancel();

        assert!(helper.cancel.is_cancelled());
    }
}
