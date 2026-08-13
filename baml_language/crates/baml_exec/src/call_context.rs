use bex_engine::{
    CancellationToken, CaptureDefaults, FunctionCallContext, FunctionCallContextBuilder, RuntimeTy,
    value_capture::TraceCaptureProducer,
};
use indexmap::IndexMap;

/// Capture settings inherited by helper calls made while dispatching a target.
///
/// JSON argument and output conversion run as distinct engine calls, so they
/// need fresh call contexts. Keeping just the capture settings lets those calls
/// share the target's capture stream without reusing its call or boundary IDs.
#[derive(Clone)]
pub struct CallContextCapture {
    capture_defaults: CaptureDefaults,
    value_capture: TraceCaptureProducer,
    cancel: CancellationToken,
}

impl CallContextCapture {
    #[must_use]
    pub fn from_call_context(context: &FunctionCallContext) -> Self {
        Self {
            capture_defaults: context.boundary.capture_defaults.clone(),
            value_capture: context.value_capture.clone(),
            cancel: context.cancel.clone(),
        }
    }

    #[must_use]
    pub fn disabled() -> Self {
        Self {
            capture_defaults: CaptureDefaults::disabled(),
            value_capture: TraceCaptureProducer::disabled(),
            cancel: CancellationToken::new(),
        }
    }

    #[must_use]
    pub(crate) fn call_context(
        &self,
        type_args: IndexMap<String, RuntimeTy>,
    ) -> FunctionCallContext {
        FunctionCallContextBuilder::new(bex_engine::CallId::next())
            .with_capture_defaults(self.capture_defaults.clone())
            .with_value_capture(self.value_capture.clone())
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
        let capture = CallContextCapture::from_call_context(&parent);
        let helper = capture.call_context(IndexMap::new());

        parent.cancel.cancel();

        assert!(helper.cancel.is_cancelled());
    }
}
