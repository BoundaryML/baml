use anyhow::Result;
use baml_types::BamlValue;

use futures::stream::{StreamExt, TryStreamExt};
use std::ops::DerefMut;
use std::sync::Arc;
use stream_cancel::{StreamExt as CancellableStreamExt, TakeUntilIf, Trigger, Tripwire};
use tokio::sync::Mutex;

use crate::FunctionResult;

type StreamCallback = Box<dyn Fn(BamlValue) -> Result<()> + Send>;

/// Wraps a stream_cancel::Trigger with an idempotent cancel.
#[derive(Clone)]
pub struct CancelStreamTrigger {
    trigger: Arc<Mutex<Option<Trigger>>>,
}

static_assertions::assert_impl_all!(CancelStreamTrigger: Send, Sync);

impl CancelStreamTrigger {
    pub async fn cancel(&self) {
        let mut locked_trigger = self.trigger.lock().await;
        let owned_trigger = core::mem::replace(locked_trigger.deref_mut(), None);
        match owned_trigger {
            Some(trigger) => trigger.cancel(),
            None => {
                log::warn!("Failed to cancel stream: trigger is None (was it already cancelled?)")
            }
        }
    }
}

pub struct FunctionResultStream {
    inner: TakeUntilIf<BoxStream<'static, FunctionResult>, Tripwire>,
    on_event: Option<StreamCallback>,
    cancelme: CancelStreamTrigger,
}

static_assertions::assert_impl_all!(FunctionResultStream: Send);

impl FunctionResultStream {
    pub fn from(inner: BoxStream<'static, FunctionResult>) -> Self {
        let (trigger, tripwire) = Tripwire::new();
        Self {
            inner: inner.take_until_if(tripwire),
            on_event: None,
            cancelme: CancelStreamTrigger {
                trigger: Arc::new(Mutex::new(Some(trigger))),
            },
        }
    }

    pub async fn run(self) -> Result<FunctionResult> {
        self.inner
            .then(|fn_result| async {
                let parsed = BamlValue::from(fn_result.parsed_content()?);
                match self.on_event {
                    None => Ok(fn_result),
                    Some(ref cb) => match cb(parsed) {
                        Ok(_) => Ok(fn_result),
                        Err(e) => Err(e),
                    },
                }
            })
            .into_stream()
            .fold(
                Err(anyhow::anyhow!("Stream failed to start")),
                |_, event| async move { Ok(event) },
            )
            .await?
    }

    pub async fn get_cancel_trigger(&self) -> CancelStreamTrigger {
        self.cancelme.clone()
    }
}
