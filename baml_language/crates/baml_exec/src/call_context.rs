use bex_engine::{
    CancellationToken, FunctionCallContext, FunctionCallContextBuilder, RuntimeTy,
    logger::TraceLogger,
};
use indexmap::IndexMap;

/// Logger and cancellation settings inherited by helper calls.
///
/// JSON argument and output conversion run as distinct engine calls, so they
/// need fresh call contexts. They are host plumbing around the user's root,
/// not user work: their roots are `SuppressInternal` so an invocation
/// publishes exactly one profiling run (the user boundary). A user-written
/// `from_json` override executed during argument decoding runs inside this
/// suppressed root; the same override called from the user's function body is
/// profiled under the user's root as usual.
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
            .suppress_internal_profile()
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

    #[test]
    fn helper_context_suppresses_internal_profile() {
        let parent = FunctionCallContextBuilder::new(bex_engine::CallId::next()).build();
        let helper = HelperCallContext::from_call_context(&parent).call_context(IndexMap::new());

        assert_eq!(
            helper.profile_intent,
            bex_engine::RootProfileIntent::SuppressInternal
        );
    }
}
