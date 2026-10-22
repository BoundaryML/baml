//! Live `FunctionSpec` / typed stream capabilities and portable prompts.

use std::{convert::Infallible, marker::PhantomData, sync::Arc};

use crate::{
    BamlValue, DecodeError, Error,
    baml_value::internal::__BamlValuePrivate,
    wire::{self, baml_outbound_value::Value as Out, inbound_map_entry::Key},
};

struct CapabilityHandle {
    key: u64,
    handle_type: wire::BamlHandleType,
    #[cfg(test)]
    release: Option<Arc<dyn Fn(u64) + Send + Sync>>,
}

impl Drop for CapabilityHandle {
    fn drop(&mut self) {
        if self.key == 0 {
            return;
        }
        #[cfg(test)]
        if let Some(release) = &self.release {
            release(self.key);
            return;
        }
        if let Ok(api) = crate::capi::api() {
            #[expect(unsafe_code)]
            unsafe {
                (api.handle_release)(self.key);
            }
        }
    }
}

impl CapabilityHandle {
    fn decode(
        value: wire::BamlOutboundValue,
        expected: wire::BamlHandleType,
        expected_name: &'static str,
    ) -> Result<Arc<Self>, DecodeError> {
        Self::live_handle(value, expected, expected_name).map(|handle| Self::own(&handle, expected))
    }

    /// The live handle of kind `expected` that `value` carries, not yet
    /// owned: a caller checks what else it needs before taking ownership.
    fn live_handle(
        value: wire::BamlOutboundValue,
        expected: wire::BamlHandleType,
        expected_name: &'static str,
    ) -> Result<wire::BamlOutboundHandle, DecodeError> {
        let value = crate::decode::unwrap(value);
        let got = crate::baml_value::wire_variant_kind(&value);
        let Some(Out::HandleValue(handle)) = value.value else {
            return Err(DecodeError::WrongType {
                expected: expected_name,
                got,
            });
        };
        if handle.key == 0 || handle.handle_type != expected as i32 {
            return Err(DecodeError::WrongType {
                expected: expected_name,
                got: "handle",
            });
        }
        Ok(handle)
    }

    /// Take ownership of a handle [`Self::live_handle`] accepted as `kind`.
    fn own(handle: &wire::BamlOutboundHandle, kind: wire::BamlHandleType) -> Arc<Self> {
        debug_assert_eq!(handle.handle_type, kind as i32);
        Arc::new(Self {
            key: handle.key,
            handle_type: kind,
            #[cfg(test)]
            release: None,
        })
    }

    fn to_baml(&self) -> wire::InboundValue {
        let mut cloned = 0_u64;
        let api = crate::capi::api().expect("BAML runtime must be loaded");
        #[expect(unsafe_code)]
        let status = unsafe { (api.handle_clone)(self.key, &raw mut cloned) };
        assert_eq!(status, 0, "failed to clone a BAML capability handle");
        wire::InboundValue {
            value_type: None,
            value: Some(wire::inbound_value::Value::Handle(wire::BamlHandle {
                key: cloned,
                handle_type: self.handle_type as i32,
            })),
        }
    }
}

/// The engine's own instance of a class that crosses by reference
/// (`reflect.Package`, `reflect.Session`, `reflect.CompileArtifact`): the
/// private state of the generated wrapper for such a class. It arrives as an
/// `ADT_RUNTIME_VALUE` handle whose type names the class, goes back as that
/// handle, and is owned once and shared by clones. Two instances are equal
/// when they hold the same key.
#[doc(hidden)]
#[derive(Clone)]
pub struct RuntimeInstance(Arc<CapabilityHandle>);

impl RuntimeInstance {
    const KIND: wire::BamlHandleType = wire::BamlHandleType::AdtRuntimeValue;

    /// Decode the instance of `class` (its FQN) that `value` carries.
    pub fn decode(
        value: wire::BamlOutboundValue,
        class: &'static str,
    ) -> Result<Self, DecodeError> {
        use wire::{baml_ty::Ty, baml_ty_class::Head};
        let handle = CapabilityHandle::live_handle(value, Self::KIND, class)?;
        // `ADT_RUNTIME_VALUE` also carries run-time-declared class and enum
        // instances; only the type beside the handle says which class this
        // one is.
        match handle.ty.as_ref().and_then(|ty| ty.ty.as_ref()) {
            Some(Ty::ClassTy(wire::BamlTyClass {
                head: Some(Head::Name(name)),
                type_args,
            })) if type_args.is_empty() => {
                if name != class {
                    return Err(DecodeError::FqnMismatch {
                        expected: class,
                        got: name.clone(),
                    });
                }
            }
            _ => {
                return Err(DecodeError::WrongType {
                    expected: class,
                    got: "handle",
                });
            }
        }
        Ok(Self(CapabilityHandle::own(&handle, Self::KIND)))
    }

    /// The instance as an inbound value: a fresh clone of its handle.
    pub fn to_baml(&self) -> wire::InboundValue {
        self.0.to_baml()
    }
}

impl PartialEq for RuntimeInstance {
    fn eq(&self, other: &Self) -> bool {
        self.0.key == other.0.key
    }
}

impl Eq for RuntimeInstance {}

impl std::hash::Hash for RuntimeInstance {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.key.hash(state);
    }
}

impl std::fmt::Debug for RuntimeInstance {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("RuntimeInstance")
            .field(&self.0.key)
            .finish()
    }
}

/// Optional controls for a bound spec call or generated flat stream shortcut.
///
/// Callers that hold a host-representable client value can override the
/// function's default without spelling the compiler-private `Fn@stream`
/// projection:
///
/// ```ignore
/// Fn_stream_with(args, CallOptions::new().client(client))?;
/// ```
#[derive(Default)]
#[must_use]
pub struct CallOptions {
    entries: Vec<(String, wire::InboundValue)>,
}

impl CallOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the selected LLM client for `FunctionSpec.call`,
    /// `FunctionSpec.build_request`, or a generated flat stream shortcut.
    pub fn client<T: BamlValue>(self, value: T) -> Self {
        self.argument("client", value)
    }

    /// Add a named control understood by the canonical `FunctionSpec` method.
    /// This is primarily an extension seam for generated controls whose host
    /// type is representable by the Rust SDK.
    #[allow(clippy::needless_pass_by_value)]
    pub fn argument<T: BamlValue>(mut self, name: impl Into<String>, value: T) -> Self {
        self.entries.push((name.into(), value.to_baml()));
        self
    }

    #[doc(hidden)]
    pub fn append_to(self, kwargs: &mut Vec<wire::InboundMapEntry>) {
        kwargs.extend(
            self.entries
                .into_iter()
                .map(|(name, value)| wire::InboundMapEntry {
                    key: Some(Key::StringKey(name)),
                    value: Some(value),
                }),
        );
    }

    fn into_kwargs(self, subject: wire::InboundValue) -> Vec<wire::InboundMapEntry> {
        std::iter::once(("self".to_string(), subject))
            .chain(self.entries)
            .map(|(name, value)| wire::InboundMapEntry {
                key: Some(Key::StringKey(name)),
                value: Some(value),
            })
            .collect()
    }
}

/// An opaque, bound `ai.FunctionSpec<Output>` recipe.
pub struct FunctionSpec<Output> {
    handle: Arc<CapabilityHandle>,
    marker: PhantomData<fn() -> Output>,
}

impl<Output> Clone for FunctionSpec<Output> {
    fn clone(&self) -> Self {
        Self {
            handle: Arc::clone(&self.handle),
            marker: PhantomData,
        }
    }
}

impl<Output> std::fmt::Debug for FunctionSpec<Output> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FunctionSpec { .. }")
    }
}

impl<Output: BamlValue> FunctionSpec<Output> {
    /// Execute the bound recipe using its default client.
    pub fn call(&self) -> Result<Output, Error<Infallible>> {
        self.call_with(CallOptions::new())
    }

    pub fn call_with(&self, options: CallOptions) -> Result<Output, Error<Infallible>> {
        crate::runtime::invoke_sync(
            "ai.FunctionSpec.call",
            options.into_kwargs(self.to_baml()),
            Vec::new(),
        )
    }

    pub async fn call_async(&self) -> Result<Output, Error<Infallible>> {
        self.call_async_with(CallOptions::new()).await
    }

    pub async fn call_async_with(&self, options: CallOptions) -> Result<Output, Error<Infallible>> {
        crate::runtime::invoke(
            "ai.FunctionSpec.call",
            options.into_kwargs(self.to_baml()),
            Vec::new(),
        )
        .await
    }

    /// Parse an existing model response using this spec's realized output.
    pub fn parse(&self, json: impl Into<String>) -> Result<Output, Error<Infallible>> {
        let json = json.into();
        crate::runtime::invoke_sync(
            "ai.FunctionSpec.parse",
            crate::encode::kwargs(vec![
                ("self", Some(self.to_baml())),
                ("json", Some(json.to_baml())),
            ]),
            Vec::new(),
        )
    }

    pub async fn parse_async(&self, json: impl Into<String>) -> Result<Output, Error<Infallible>> {
        let json = json.into();
        crate::runtime::invoke(
            "ai.FunctionSpec.parse",
            crate::encode::kwargs(vec![
                ("self", Some(self.to_baml())),
                ("json", Some(json.to_baml())),
            ]),
            Vec::new(),
        )
        .await
    }

    /// Render the provider-neutral, portable prompt.
    pub fn prompt(&self) -> Result<Prompt, Error<Infallible>> {
        crate::runtime::invoke_sync(
            "ai.FunctionSpec.prompt",
            crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
            Vec::new(),
        )
    }

    pub async fn prompt_async(&self) -> Result<Prompt, Error<Infallible>> {
        crate::runtime::invoke(
            "ai.FunctionSpec.prompt",
            crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
            Vec::new(),
        )
        .await
    }

    /// Build the provider request without invoking the model. The expected
    /// request type is inferred from assignment/return context.
    pub fn build_request<Request: BamlValue>(&self) -> Result<Request, Error<Infallible>> {
        self.build_request_with(CallOptions::new())
    }

    pub fn build_request_with<Request: BamlValue>(
        &self,
        options: CallOptions,
    ) -> Result<Request, Error<Infallible>> {
        crate::runtime::invoke_sync(
            "ai.FunctionSpec.build_request",
            options.into_kwargs(self.to_baml()),
            Vec::new(),
        )
    }

    pub async fn build_request_async<Request: BamlValue>(
        &self,
    ) -> Result<Request, Error<Infallible>> {
        self.build_request_async_with(CallOptions::new()).await
    }

    pub async fn build_request_async_with<Request: BamlValue>(
        &self,
        options: CallOptions,
    ) -> Result<Request, Error<Infallible>> {
        crate::runtime::invoke(
            "ai.FunctionSpec.build_request",
            options.into_kwargs(self.to_baml()),
            Vec::new(),
        )
        .await
    }

    pub fn name(&self) -> Result<String, Error<Infallible>> {
        crate::runtime::invoke_sync(
            "ai.FunctionSpec.name",
            crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
            Vec::new(),
        )
    }

    pub async fn name_async(&self) -> Result<String, Error<Infallible>> {
        crate::runtime::invoke(
            "ai.FunctionSpec.name",
            crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
            Vec::new(),
        )
        .await
    }
}

impl<Output: BamlValue> __BamlValuePrivate for FunctionSpec<Output> {
    fn to_baml(&self) -> wire::InboundValue {
        self.handle.to_baml()
    }

    fn from_baml(value: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        Ok(Self {
            handle: CapabilityHandle::decode(
                value,
                wire::BamlHandleType::AdtFunctionSpec,
                "ai.FunctionSpec handle",
            )?,
            marker: PhantomData,
        })
    }

    fn baml_ty() -> wire::BamlTy {
        crate::baml_value::internal::class_ty("ai.FunctionSpec", vec![Output::baml_ty()])
    }
}

/// A live `ai.stream.Stream<T>` capability. A partial and the settled value
/// share the one type: a partial is `T` parsed from the text received so far.
pub struct Stream<T> {
    handle: Arc<CapabilityHandle>,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for Stream<T> {
    fn clone(&self) -> Self {
        Self {
            handle: Arc::clone(&self.handle),
            marker: PhantomData,
        }
    }
}

impl<T> std::fmt::Debug for Stream<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Stream { .. }")
    }
}

impl<T: BamlValue> Stream<T> {
    /// Yield one partial, or `None` for the distinct `ai.stream.Done`
    /// sentinel. A nullable partial is therefore `Some(None)`, not done.
    pub fn next(&self) -> Result<Option<T>, Error<Infallible>> {
        let value: wire::BamlOutboundValue = crate::runtime::invoke_sync(
            "ai.stream.Stream.next",
            crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
            Vec::new(),
        )?;
        decode_stream_item(value).map_err(Error::Decode)
    }

    pub async fn next_async(&self) -> Result<Option<T>, Error<Infallible>> {
        let value: wire::BamlOutboundValue = crate::runtime::invoke(
            "ai.stream.Stream.next",
            crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
            Vec::new(),
        )
        .await?;
        decode_stream_item(value).map_err(Error::Decode)
    }

    /// Drain the stream and return its settled output.
    pub fn final_(&self) -> Result<T, Error<Infallible>> {
        crate::runtime::invoke_sync(
            "ai.stream.Stream.final",
            crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
            Vec::new(),
        )
    }

    pub async fn final_async(&self) -> Result<T, Error<Infallible>> {
        crate::runtime::invoke(
            "ai.stream.Stream.final",
            crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
            Vec::new(),
        )
        .await
    }
}

fn decode_stream_item<T: BamlValue>(
    value: wire::BamlOutboundValue,
) -> Result<Option<T>, DecodeError> {
    let unwrapped = crate::decode::unwrap(value.clone());
    if matches!(
        unwrapped.value,
        Some(Out::ClassValue(ref class)) if class.name == "ai.stream.Done"
    ) {
        Ok(None)
    } else {
        T::from_baml(value).map(Some)
    }
}

impl<T: BamlValue> __BamlValuePrivate for Stream<T> {
    fn to_baml(&self) -> wire::InboundValue {
        self.handle.to_baml()
    }

    fn from_baml(value: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        Ok(Self {
            handle: CapabilityHandle::decode(
                value,
                wire::BamlHandleType::AdtTaggedHeapHandle,
                "ai.stream.Stream handle",
            )?,
            marker: PhantomData,
        })
    }

    fn baml_ty() -> wire::BamlTy {
        crate::baml_value::internal::class_ty("ai.stream.Stream", vec![T::baml_ty()])
    }
}

/// Owned provider-neutral prompt data. Unlike a spec or stream, this value is
/// portable and may be cloned, persisted by a protobuf-aware host, and passed
/// back into any compatible runtime.
#[derive(Clone, PartialEq)]
pub struct Prompt {
    value: wire::BamlValuePromptAst,
}

impl std::fmt::Debug for Prompt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Prompt { .. }")
    }
}

impl Prompt {
    pub fn text(&self) -> Result<String, Error<Infallible>> {
        crate::runtime::invoke_sync(
            "ai.Prompt.text",
            crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
            Vec::new(),
        )
    }

    pub async fn text_async(&self) -> Result<String, Error<Infallible>> {
        crate::runtime::invoke(
            "ai.Prompt.text",
            crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
            Vec::new(),
        )
        .await
    }

    /// Decode the structural message view selected by the caller's expected
    /// generated type.
    pub fn messages<Messages: BamlValue>(&self) -> Result<Messages, Error<Infallible>> {
        crate::runtime::invoke_sync(
            "ai.Prompt.messages",
            crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
            Vec::new(),
        )
    }
}

impl __BamlValuePrivate for Prompt {
    fn to_baml(&self) -> wire::InboundValue {
        wire::InboundValue {
            value_type: None,
            value: Some(wire::inbound_value::Value::PromptAstValue(
                self.value.clone(),
            )),
        }
    }

    fn from_baml(value: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let value = crate::decode::unwrap(value);
        let got = crate::baml_value::wire_variant_kind(&value);
        match value.value {
            Some(Out::PromptAstValue(value)) => Ok(Self { value }),
            _ => Err(DecodeError::WrongType {
                expected: "ai.Prompt",
                got,
            }),
        }
    }

    fn baml_ty() -> wire::BamlTy {
        wire::BamlTy {
            ty: Some(wire::baml_ty::Ty::PromptAst(wire::BamlTyPromptAst {})),
        }
    }
}

// The stream bridge first decodes its union result as a raw outbound value so
// it can distinguish `Done` from a nullable partial.
impl __BamlValuePrivate for wire::BamlOutboundValue {
    fn to_baml(&self) -> wire::InboundValue {
        panic!("raw outbound values cannot be passed into BAML")
    }

    fn from_baml(value: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        Ok(value)
    }

    fn baml_ty() -> wire::BamlTy {
        wire::BamlTy {
            ty: Some(wire::baml_ty::Ty::Unknown(wire::BamlTyUnknown {})),
        }
    }
}

impl<Output: BamlValue> FunctionSpec<Output> {
    pub fn call_with_options(
        &self,
        args: CallOptions,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Output, Error<Infallible>> {
        crate::runtime::invoke_sync_with_options(
            "ai.FunctionSpec.call",
            || (args.into_kwargs(self.to_baml()), Vec::new()),
            baml.into(),
        )
    }
    pub async fn call_async_with_options(
        &self,
        args: CallOptions,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Output, Error<Infallible>> {
        crate::runtime::invoke_with_options(
            "ai.FunctionSpec.call",
            || (args.into_kwargs(self.to_baml()), Vec::new()),
            baml.into(),
        )
        .await
    }
    pub fn parse_with_options(
        &self,
        json: impl Into<String>,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Output, Error<Infallible>> {
        crate::runtime::invoke_sync_with_options(
            "ai.FunctionSpec.parse",
            || {
                (
                    crate::encode::kwargs(vec![
                        ("self", Some(self.to_baml())),
                        ("json", Some(json.into().to_baml())),
                    ]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
    }
    pub async fn parse_async_with_options(
        &self,
        json: impl Into<String>,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Output, Error<Infallible>> {
        crate::runtime::invoke_with_options(
            "ai.FunctionSpec.parse",
            || {
                (
                    crate::encode::kwargs(vec![
                        ("self", Some(self.to_baml())),
                        ("json", Some(json.into().to_baml())),
                    ]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
        .await
    }
    pub fn prompt_with_options(
        &self,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Prompt, Error<Infallible>> {
        crate::runtime::invoke_sync_with_options(
            "ai.FunctionSpec.prompt",
            || {
                (
                    crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
    }
    pub async fn prompt_async_with_options(
        &self,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Prompt, Error<Infallible>> {
        crate::runtime::invoke_with_options(
            "ai.FunctionSpec.prompt",
            || {
                (
                    crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
        .await
    }
    pub fn build_request_with_options<Request: BamlValue>(
        &self,
        args: CallOptions,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Request, Error<Infallible>> {
        crate::runtime::invoke_sync_with_options(
            "ai.FunctionSpec.build_request",
            || (args.into_kwargs(self.to_baml()), Vec::new()),
            baml.into(),
        )
    }
    pub async fn build_request_async_with_options<Request: BamlValue>(
        &self,
        args: CallOptions,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Request, Error<Infallible>> {
        crate::runtime::invoke_with_options(
            "ai.FunctionSpec.build_request",
            || (args.into_kwargs(self.to_baml()), Vec::new()),
            baml.into(),
        )
        .await
    }
    pub fn name_with_options(
        &self,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<String, Error<Infallible>> {
        crate::runtime::invoke_sync_with_options(
            "ai.FunctionSpec.name",
            || {
                (
                    crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
    }
    pub async fn name_async_with_options(
        &self,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<String, Error<Infallible>> {
        crate::runtime::invoke_with_options(
            "ai.FunctionSpec.name",
            || {
                (
                    crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
        .await
    }
}
impl<T: BamlValue> Stream<T> {
    pub fn next_with_options(
        &self,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Option<T>, Error<Infallible>> {
        let value: wire::BamlOutboundValue = crate::runtime::invoke_sync_with_options(
            "ai.stream.Stream.next",
            || {
                (
                    crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
                    Vec::new(),
                )
            },
            baml.into(),
        )?;
        decode_stream_item(value).map_err(Error::Decode)
    }
    pub async fn next_async_with_options(
        &self,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Option<T>, Error<Infallible>> {
        let value: wire::BamlOutboundValue = crate::runtime::invoke_with_options(
            "ai.stream.Stream.next",
            || {
                (
                    crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
        .await?;
        decode_stream_item(value).map_err(Error::Decode)
    }
    pub fn final_with_options(
        &self,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<T, Error<Infallible>> {
        crate::runtime::invoke_sync_with_options(
            "ai.stream.Stream.final",
            || {
                (
                    crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
    }
    pub async fn final_async_with_options(
        &self,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<T, Error<Infallible>> {
        crate::runtime::invoke_with_options(
            "ai.stream.Stream.final",
            || {
                (
                    crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
        .await
    }
}
impl Prompt {
    pub fn text_with_options(
        &self,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<String, Error<Infallible>> {
        crate::runtime::invoke_sync_with_options(
            "ai.Prompt.text",
            || {
                (
                    crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
    }
    pub async fn text_async_with_options(
        &self,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<String, Error<Infallible>> {
        crate::runtime::invoke_with_options(
            "ai.Prompt.text",
            || {
                (
                    crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
        .await
    }
    pub fn messages_with_options<Messages: BamlValue>(
        &self,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Messages, Error<Infallible>> {
        crate::runtime::invoke_sync_with_options(
            "ai.Prompt.messages",
            || {
                (
                    crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
    }
    pub async fn messages_async_with_options<Messages: BamlValue>(
        &self,
        baml: impl Into<crate::invocation::InvocationOptions>,
    ) -> Result<Messages, Error<Infallible>> {
        crate::runtime::invoke_with_options(
            "ai.Prompt.messages",
            || {
                (
                    crate::encode::kwargs(vec![("self", Some(self.to_baml()))]),
                    Vec::new(),
                )
            },
            baml.into(),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    #[test]
    fn capability_clones_share_one_owned_handle() {
        let released = Arc::new(AtomicU64::new(0));
        let capture = Arc::clone(&released);
        let spec = FunctionSpec::<String> {
            handle: Arc::new(CapabilityHandle {
                key: 91,
                handle_type: wire::BamlHandleType::AdtFunctionSpec,
                release: Some(Arc::new(move |key| capture.store(key, Ordering::SeqCst))),
            }),
            marker: PhantomData,
        };
        let clone = spec.clone();
        drop(spec);
        assert_eq!(released.load(Ordering::SeqCst), 0);
        drop(clone);
        assert_eq!(released.load(Ordering::SeqCst), 91);
    }

    /// An outbound handle of `kind` whose type names the class `head`.
    fn instance_handle(
        key: u64,
        kind: wire::BamlHandleType,
        head: &str,
    ) -> wire::BamlOutboundValue {
        wire::BamlOutboundValue {
            value: Some(Out::HandleValue(wire::BamlOutboundHandle {
                key,
                handle_type: kind as i32,
                ty: Some(crate::baml_value::internal::class_ty(head, Vec::new())),
            })),
        }
    }

    /// Decode a `reflect.Package` instance whose release this test observes
    /// instead of the engine.
    fn decode_package(key: u64) -> Result<RuntimeInstance, DecodeError> {
        let mut instance = RuntimeInstance::decode(
            instance_handle(
                key,
                wire::BamlHandleType::AdtRuntimeValue,
                "reflect.Package",
            ),
            "reflect.Package",
        )?;
        Arc::get_mut(&mut instance.0)
            .expect("a freshly decoded instance has one owner")
            .release = Some(Arc::new(|_| {}));
        Ok(instance)
    }

    #[test]
    fn runtime_instances_decode_from_the_handle_naming_their_class() {
        let package = decode_package(7).unwrap();
        assert_eq!(package.0.key, 7);
        assert_eq!(package.0.handle_type, wire::BamlHandleType::AdtRuntimeValue);

        // The kind is shared with run-time-declared instances: the type
        // beside the handle decides the class.
        assert_eq!(
            RuntimeInstance::decode(
                instance_handle(8, wire::BamlHandleType::AdtRuntimeValue, "reflect.Session"),
                "reflect.Package",
            )
            .unwrap_err(),
            DecodeError::FqnMismatch {
                expected: "reflect.Package",
                got: "reflect.Session".to_string(),
            }
        );
        // Any other handle kind, and the fields of a class value, are not
        // the instance.
        assert_eq!(
            RuntimeInstance::decode(
                instance_handle(9, wire::BamlHandleType::UntaggedBexHeap, "reflect.Package"),
                "reflect.Package",
            )
            .unwrap_err(),
            DecodeError::WrongType {
                expected: "reflect.Package",
                got: "handle",
            }
        );
        let by_fields = wire::BamlOutboundValue {
            value: Some(Out::ClassValue(wire::BamlValueClass {
                name: "reflect.Package".to_string(),
                type_args: Vec::new(),
                fields: Vec::new(),
            })),
        };
        assert_eq!(
            RuntimeInstance::decode(by_fields, "reflect.Package").unwrap_err(),
            DecodeError::WrongType {
                expected: "reflect.Package",
                got: "class",
            }
        );
    }

    #[test]
    fn runtime_instances_are_equal_by_key() {
        use std::hash::{BuildHasher, RandomState};

        let package = decode_package(7).unwrap();
        let clone = package.clone();
        let other = decode_package(8).unwrap();
        assert_eq!(package, clone);
        assert_ne!(package, other);
        let hasher = RandomState::new();
        assert_eq!(hasher.hash_one(&package), hasher.hash_one(&clone));
    }

    #[test]
    fn done_is_distinct_from_a_nullable_partial() {
        let done = wire::BamlOutboundValue {
            value: Some(Out::ClassValue(wire::BamlValueClass {
                name: "ai.stream.Done".to_string(),
                type_args: Vec::new(),
                fields: Vec::new(),
            })),
        };
        assert_eq!(decode_stream_item::<Option<String>>(done).unwrap(), None);

        let null = wire::BamlOutboundValue {
            value: Some(Out::NullValue(wire::BamlValueNull {})),
        };
        assert_eq!(
            decode_stream_item::<Option<String>>(null).unwrap(),
            Some(None)
        );
    }
}
