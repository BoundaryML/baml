use std::sync::mpsc;

use prost::Message;

use crate::{
    args::cancellation::OnCancelGuard, codec::BamlDecode, error::BamlError,
    ffi::callbacks::CallbackResult, proto::baml_cffi_v1::CffiValueHolder,
};

/// Event from a streaming function call
pub enum StreamEvent<TPartial, TFinal> {
    /// Partial result during streaming
    Partial(TPartial),
    /// Final complete result
    Final(TFinal),
    /// Error occurred
    Error(BamlError),
}

enum StreamState {
    Open,
    Finished,
}

/// Result of a streaming function call
pub struct StreamingCall<TStream, TFinal: Clone> {
    id: u32,
    receiver: mpsc::Receiver<CallbackResult>,
    state: StreamState,
    final_value: Option<Result<TFinal, BamlError>>,
    // Holds the cancellation guard - when dropped, stops watching for cancellation
    _cancel_guard: Option<OnCancelGuard>,
    // Since we we need to have a dual type parameter, we use a phantom type to ensure type safety
    // Rust requires that declared generic types be used in the type parameters, so we use a
    // phantom type to ensure type safety
    _phantom: std::marker::PhantomData<TStream>,
}

enum Internal<TPartial, TFinal> {
    Partial(TPartial),
    Final(TFinal),
    Error(BamlError),
}

impl<TPartial, TFinal: Clone> StreamingCall<TPartial, TFinal>
where
    TPartial: BamlDecode,
    TFinal: BamlDecode,
{
    pub(crate) fn new(
        id: u32,
        receiver: mpsc::Receiver<CallbackResult>,
        cancel_guard: Option<OnCancelGuard>,
    ) -> Self {
        Self {
            id,
            receiver,
            state: StreamState::Open,
            final_value: None,
            _cancel_guard: cancel_guard,
            _phantom: std::marker::PhantomData,
        }
    }

    fn recv_internal(&mut self) -> Option<Result<Internal<TPartial, TFinal>, BamlError>> {
        if matches!(self.state, StreamState::Finished) {
            return None;
        }

        match self.receiver.recv() {
            Ok(CallbackResult::Partial(bytes)) => {
                Some(decode_partial(&bytes).map(|p| Internal::Partial(p)))
            }

            Ok(CallbackResult::Final(bytes)) => {
                self.state = StreamState::Finished;
                let decoded = decode_final(&bytes);
                self.final_value = Some(decoded.clone());
                Some(decoded.map(|v| Internal::Final(v)))
            }

            Ok(CallbackResult::Error(e)) => {
                self.state = StreamState::Finished;
                self.final_value = Some(Err(e.clone()));
                Some(Ok(Internal::Error(e)))
            }

            Err(_) => {
                self.state = StreamState::Finished;
                Some(Err(BamlError::internal("callback channel closed")))
            }
        }
    }

    pub fn iter_mut(&mut self) -> Partials<'_, TPartial, TFinal> {
        <&mut Self as IntoIterator>::into_iter(self)
    }
}

fn decode_partial<T: BamlDecode>(data: &[u8]) -> Result<T, BamlError> {
    let holder = CffiValueHolder::decode(data)
        .map_err(|e| BamlError::internal(format!("decode error: {e}")))?;
    T::baml_decode(&holder)
}

fn decode_final<T: BamlDecode>(data: &[u8]) -> Result<T, BamlError> {
    let holder = CffiValueHolder::decode(data)
        .map_err(|e| BamlError::internal(format!("decode error: {e}")))?;
    T::baml_decode(&holder)
}

pub struct Partials<'a, TPartial, TFinal: Clone> {
    call: &'a mut StreamingCall<TPartial, TFinal>,
}

impl<TPartial, TFinal: Clone> Iterator for Partials<'_, TPartial, TFinal>
where
    TPartial: BamlDecode,
    TFinal: BamlDecode,
{
    type Item = Result<TPartial, BamlError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.call.recv_internal() {
            Some(Ok(Internal::Partial(p))) => Some(Ok(p)),
            Some(Ok(Internal::Error(e))) => Some(Err(e)),
            Some(Err(e)) => Some(Err(e)),
            Some(Ok(Internal::Final(_))) => None,
            None => None,
        }
    }
}

/// Public interface for the streaming call
impl<TPartial, TFinal: Clone> StreamingCall<TPartial, TFinal>
where
    TPartial: BamlDecode,
    TFinal: BamlDecode,
{
    /// Iterate progress updates (partials only)
    pub fn partials(&mut self) -> Partials<'_, TPartial, TFinal> {
        Partials { call: self }
    }

    pub fn cancel(&mut self) {
        #[allow(unsafe_code)]
        unsafe {
            let _ = baml_sys::cancel_function_call(self.id);
        }
    }

    /// Block until final result (discarding partials)
    pub fn get_final_response(mut self) -> Result<TFinal, BamlError> {
        if let Some(res) = self.final_value.take() {
            return res;
        }

        loop {
            match self.recv_internal() {
                Some(Ok(Internal::Partial(_))) => continue,
                Some(Ok(Internal::Final(v))) => return Ok(v),
                Some(Ok(Internal::Error(e))) => return Err(e),
                Some(Err(e)) => return Err(e),
                None => return Err(BamlError::internal("callback channel closed")),
            }
        }
    }
}

/// Support for iterating over the streaming call
impl<'a, TPartial, TFinal> IntoIterator for &'a mut StreamingCall<TPartial, TFinal>
where
    TPartial: BamlDecode,
    TFinal: Clone + BamlDecode,
{
    type IntoIter = Partials<'a, TPartial, TFinal>;
    type Item = Result<TPartial, BamlError>;

    fn into_iter(self) -> Self::IntoIter {
        self.partials()
    }
}

// Support for owned for loops

pub struct PartialsOwned<TPartial, TFinal: Clone> {
    call: StreamingCall<TPartial, TFinal>,
}

impl<TPartial, TFinal> Iterator for PartialsOwned<TPartial, TFinal>
where
    TPartial: BamlDecode,
    TFinal: Clone + BamlDecode,
{
    type Item = Result<TPartial, BamlError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.call.recv_internal() {
            Some(Ok(Internal::Partial(p))) => Some(Ok(p)),
            Some(Ok(Internal::Final(_))) => None,
            Some(Ok(Internal::Error(e))) => Some(Err(e)),
            Some(Err(e)) => Some(Err(e)),
            None => None,
        }
    }
}

impl<TPartial, TFinal: Clone> IntoIterator for StreamingCall<TPartial, TFinal>
where
    TPartial: BamlDecode,
    TFinal: BamlDecode,
{
    type IntoIter = PartialsOwned<TPartial, TFinal>;
    type Item = Result<TPartial, BamlError>;

    fn into_iter(self) -> Self::IntoIter {
        PartialsOwned { call: self }
    }
}

impl<TPartial, TFinal: Clone> Drop for StreamingCall<TPartial, TFinal> {
    fn drop(&mut self) {
        if matches!(self.state, StreamState::Open) {
            #[allow(unsafe_code)]
            unsafe {
                let _ = baml_sys::cancel_function_call(self.id);
            }
        }
    }
}
