use std::sync::mpsc;

use prost::Message;

use crate::codec::BamlDecode;
use crate::error::BamlError;
use crate::ffi::callbacks::CallbackResult;
use crate::proto::baml_cffi_v1::CffiValueHolder;

/// Event from a streaming function call
pub enum StreamEvent<TPartial, TFinal> {
    /// Partial result during streaming
    Partial(TPartial),
    /// Final complete result
    Final(TFinal),
    /// Error occurred
    Error(BamlError),
}

/// Result of a streaming function call
pub struct StreamResult<TPartial, TFinal> {
    receiver: mpsc::Receiver<CallbackResult>,
    _phantom: std::marker::PhantomData<(TPartial, TFinal)>,
}

impl<TPartial, TFinal> StreamResult<TPartial, TFinal>
where
    TPartial: BamlDecode,
    TFinal: BamlDecode,
{
    pub(crate) fn new(receiver: mpsc::Receiver<CallbackResult>) -> Self {
        Self {
            receiver,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Blocking receive - waits for next event
    /// Returns None when stream is complete
    pub fn recv(&self) -> Option<StreamEvent<TPartial, TFinal>> {
        match self.receiver.recv() {
            Ok(CallbackResult::Partial(data)) => match decode_partial::<TPartial>(&data) {
                Ok(val) => Some(StreamEvent::Partial(val)),
                Err(e) => Some(StreamEvent::Error(e)),
            },
            Ok(CallbackResult::Final(data)) => match decode_final::<TFinal>(&data) {
                Ok(val) => Some(StreamEvent::Final(val)),
                Err(e) => Some(StreamEvent::Error(e)),
            },
            Ok(CallbackResult::Error(e)) => Some(StreamEvent::Error(e)),
            Err(_) => None, // Channel closed
        }
    }

    /// Try to get next event without blocking
    pub fn try_recv(&self) -> Option<StreamEvent<TPartial, TFinal>> {
        match self.receiver.try_recv() {
            Ok(CallbackResult::Partial(data)) => match decode_partial::<TPartial>(&data) {
                Ok(val) => Some(StreamEvent::Partial(val)),
                Err(e) => Some(StreamEvent::Error(e)),
            },
            Ok(CallbackResult::Final(data)) => match decode_final::<TFinal>(&data) {
                Ok(val) => Some(StreamEvent::Final(val)),
                Err(e) => Some(StreamEvent::Error(e)),
            },
            Ok(CallbackResult::Error(e)) => Some(StreamEvent::Error(e)),
            Err(_) => None,
        }
    }

    /// Get only the final result, blocking until complete (discards partials)
    pub fn final_result(self) -> Result<TFinal, BamlError> {
        loop {
            match self.recv() {
                Some(StreamEvent::Partial(_)) => continue,
                Some(StreamEvent::Final(val)) => return Ok(val),
                Some(StreamEvent::Error(e)) => return Err(e),
                None => return Err(BamlError::internal("stream ended without final result")),
            }
        }
    }
}

/// Iterator over stream events
impl<TPartial, TFinal> Iterator for StreamResult<TPartial, TFinal>
where
    TPartial: BamlDecode,
    TFinal: BamlDecode,
{
    type Item = StreamEvent<TPartial, TFinal>;

    fn next(&mut self) -> Option<Self::Item> {
        self.recv()
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
