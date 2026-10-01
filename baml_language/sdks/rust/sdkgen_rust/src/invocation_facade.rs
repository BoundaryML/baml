// Generated invocation controls use the SDK's actual stdlib projections.
use crate::baml::spawn::CancelToken;
use crate::vendor::trace::{Options, ReservedSpan, Selection};
use baml_bridge::baml_value::internal::__BamlValuePrivate;

#[derive(Clone, Default)]
pub struct BamlOptions {
    trace: Option<Selection>,
    cancel: Option<CancelToken>,
    timeout_ms: Option<u32>,
}
impl BamlOptions {
    pub fn new() -> Self { Self::default() }
    pub fn trace(mut self, trace: impl Into<Selection>) -> Self { self.trace = Some(trace.into()); self }
    pub fn cancel(mut self, token: CancelToken) -> Self { self.cancel = Some(token); self }
    pub fn timeout_ms(mut self, timeout: u32) -> Self { self.timeout_ms = Some(timeout); self }
}
impl From<&BamlOptions> for BamlOptions { fn from(value: &BamlOptions) -> Self { value.clone() } }
impl From<Options> for BamlOptions { fn from(value: Options) -> Self { Self::new().trace(value) } }
impl From<&Options> for BamlOptions { fn from(value: &Options) -> Self { Self::new().trace(value.clone()) } }
impl From<ReservedSpan> for BamlOptions { fn from(value: ReservedSpan) -> Self { Self::new().trace(value) } }
impl From<&ReservedSpan> for BamlOptions { fn from(value: &ReservedSpan) -> Self { Self::new().trace(value.clone()) } }
impl From<Selection> for BamlOptions { fn from(value: Selection) -> Self { Self::new().trace(value) } }
impl From<&Selection> for BamlOptions { fn from(value: &Selection) -> Self { Self::new().trace(value.clone()) } }
impl From<CancelToken> for BamlOptions { fn from(value: CancelToken) -> Self { Self::new().cancel(value) } }
impl From<&CancelToken> for BamlOptions { fn from(value: &CancelToken) -> Self { Self::new().cancel(value.clone()) } }

impl From<BamlOptions> for baml_bridge::invocation::InvocationOptions {
    fn from(value: BamlOptions) -> Self {
        let trace = value.trace.map(|trace| match trace { Selection::Options(value) => value._handle, Selection::ReservedSpan(value) => value._handle });
        let cancel = value.cancel.map(|cancel| std::sync::Arc::new(move || cancel.to_baml()) as std::sync::Arc<dyn Fn() -> baml_bridge::wire::InboundValue + Send + Sync>);
        Self::new(trace, cancel, value.timeout_ms)
    }
}
impl From<&BamlOptions> for baml_bridge::invocation::InvocationOptions { fn from(value: &BamlOptions) -> Self { value.clone().into() } }
macro_rules! bridge_control {
    ($type:ty) => {
        impl From<$type> for baml_bridge::invocation::InvocationOptions { fn from(value: $type) -> Self { BamlOptions::from(value).into() } }
        impl From<&$type> for baml_bridge::invocation::InvocationOptions { fn from(value: &$type) -> Self { BamlOptions::from(value).into() } }
    }
}
bridge_control!(Options); bridge_control!(ReservedSpan); bridge_control!(Selection); bridge_control!(CancelToken);
