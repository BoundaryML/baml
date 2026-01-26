use std::marker::PhantomData;

use prost::Message;

use crate::{
    args::cancellation::OnCancelGuard,
    codec::BamlDecode,
    error::BamlError,
    ffi::{self, callbacks::CallbackResult},
    proto::baml_cffi_v1::CffiValueHolder,
};

/// State of the async streaming call
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamState {
    Open,
    Finished,
}

/// Async streaming call that yields partial results and a final response
pub struct AsyncStreamingCall<TPartial, TFinal> {
    id: u32,
    receiver: async_channel::Receiver<CallbackResult>,
    state: StreamState,
    final_value: Option<Result<TFinal, BamlError>>,
    // Holds the cancellation guard - when dropped, stops watching for cancellation
    _cancel_guard: Option<OnCancelGuard>,
    _phantom: PhantomData<TPartial>,
}

impl<TPartial, TFinal> AsyncStreamingCall<TPartial, TFinal>
where
    TPartial: BamlDecode,
    TFinal: BamlDecode + Clone,
{
    pub(crate) fn new(
        id: u32,
        receiver: async_channel::Receiver<CallbackResult>,
        cancel_guard: Option<OnCancelGuard>,
    ) -> Self {
        Self {
            id,
            receiver,
            state: StreamState::Open,
            final_value: None,
            _cancel_guard: cancel_guard,
            _phantom: PhantomData,
        }
    }

    /// Get the next partial result.
    ///
    /// Returns `Some(Ok(partial))` for each streaming update.
    /// Returns `Some(Err(e))` on error.
    /// Returns `None` when the stream is complete (final result received).
    ///
    /// After receiving `None`, call `get_final_response()` to get the final
    /// result.
    pub async fn next(&mut self) -> Option<Result<TPartial, BamlError>> {
        if self.state == StreamState::Finished {
            return None;
        }

        match self.receiver.recv().await {
            Ok(CallbackResult::Partial(bytes)) => Some(decode_partial(&bytes)),
            Ok(CallbackResult::Final(bytes)) => {
                self.state = StreamState::Finished;
                self.final_value = Some(decode_final(&bytes));
                None
            }
            Ok(CallbackResult::Error(e)) => {
                self.state = StreamState::Finished;
                self.final_value = Some(Err(e.clone()));
                Some(Err(e))
            }
            Err(_) => {
                self.state = StreamState::Finished;
                Some(Err(BamlError::internal("callback channel closed")))
            }
        }
    }

    /// Get the final response, consuming the stream.
    ///
    /// If there are remaining partial results, they will be drained.
    /// Call this after `next()` returns `None`, or to skip all partials and get
    /// just the final.
    pub async fn get_final_response(mut self) -> Result<TFinal, BamlError> {
        // Return cached final if we already have it
        if let Some(result) = self.final_value.take() {
            return result;
        }

        // Drain remaining partials until we get the final
        loop {
            match self.receiver.recv().await {
                Ok(CallbackResult::Partial(_)) => continue,
                Ok(CallbackResult::Final(bytes)) => {
                    self.state = StreamState::Finished;
                    return decode_final(&bytes);
                }
                Ok(CallbackResult::Error(e)) => {
                    self.state = StreamState::Finished;
                    return Err(e);
                }
                Err(_) => {
                    self.state = StreamState::Finished;
                    return Err(BamlError::internal("callback channel closed"));
                }
            }
        }
    }

    /// Check if the stream is finished
    pub fn is_finished(&self) -> bool {
        self.state == StreamState::Finished
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

/// Cancellation on drop - if stream is dropped before completion, cancel the
/// underlying call
impl<TPartial, TFinal> Drop for AsyncStreamingCall<TPartial, TFinal> {
    fn drop(&mut self) {
        if self.state == StreamState::Open {
            // Cancel the FFI operation
            #[allow(unsafe_code)]
            unsafe {
                let _ = ffi::cancel_function_call(self.id);
            }
        }
    }
}
