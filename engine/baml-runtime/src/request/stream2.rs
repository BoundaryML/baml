use anyhow::Result;
use eventsource_stream::Eventsource;
use futures::channel::oneshot;
use futures::stream::StreamExt;
use reqwest::Client;
use std::sync::Arc;
use stream_cancel::{StreamExt as CancellableStreamExt, Trigger, Tripwire};
use tokio::sync::Mutex;

type StreamCallback = Box<dyn Fn(String) -> Result<String>>;
//pub struct OpenAiStream<T>
//where
//    T: FnMut(String) -> Result<String>,
pub struct OpenAiStream {
    pub cancelme: Option<Box<Trigger>>,
    pub callback: Option<StreamCallback>,
    tripwire: Tripwire,
    receiver: async_std::channel::Receiver<bool>,
}

impl OpenAiStream
//impl<T> OpenAiStream<T>
//where
//    T: FnMut(String) -> Result<String>,
{
    pub fn new() -> (OpenAiStream, async_std::channel::Sender<bool>) {
        let (tx, rx) = async_std::channel::bounded::<bool>(1);
        let (trigger, tripwire) = Tripwire::new();
        (
            OpenAiStream {
                cancelme: Some(Box::new(trigger)),
                callback: None,
                tripwire,
                receiver: rx,
            },
            tx,
        )
    }

    pub fn on_event(&mut self, callback: StreamCallback) {
        self.callback = Some(callback);
    }

    pub async fn stream(mut self) -> Result<String> {
        let mut stream = reqwest::Client::new()
            .get("http://localhost:7331/events")
            .send()
            .await?
            .bytes_stream()
            .eventsource()
            .take_until_if(self.tripwire);
        //.take_until_if(std::pin::pin!(async {
        //    match self.receiver.recv().await {
        //        Ok(b) => return b,
        //        Err(e) => {
        //            log::warn!("error receiving from channel: {}", e);
        //            return true;
        //        }
        //    }
        //}));

        log::info!("stream created inside");
        let mut i = 0;
        while let Some(event) = stream.next().await {
            match event {
                Ok(event) => match self.callback {
                    Some(ref c) => {
                        log::info!("applied callback to event: {}", c(event.data)?)
                    }
                    None => log::info!("received event[type={}]: {}", event.event, event.data),
                },
                Err(e) => log::warn!("stream error occurred: {}", e),
            }
            i += 1;
            if i > 3 {
                let cancel_trigger = self.cancelme;
                self.cancelme = None;
                if let Some(trigger) = cancel_trigger {
                    drop(trigger);
                }
            }
        }
        log::info!("stream end inside");

        Ok("lorem ipsum dolor".into())
    }

    pub fn cancel(&self) {
        //self.tx.send(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::time::Duration;
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    #[wasm_bindgen_test]
    pub async fn do_test() -> Result<()> {
        wasm_bindgen_test_configure!(run_in_browser);
        console_log::init_with_level(log::Level::Debug)?;
        log::info!("test started");

        let (mut stream, tx_cancel) = OpenAiStream::new();

        log::info!("stream created");
        let cancel_stream = async move {
            log::info!("cancelling stream after 5s");
            async_std::task::sleep(Duration::from_millis(1000)).await;
            log::info!("1s elapsed for canceller");
            async_std::task::sleep(Duration::from_millis(2000)).await;
            log::info!("3s elapsed for canceller");
            async_std::task::sleep(Duration::from_millis(1500)).await;
            tx_cancel.send(true);
            log::info!("cancelled stream after 5s");
        };

        let (_, final_output) = futures::join!(cancel_stream, stream.stream());
        log::info!("stream end with final={:#?}", final_output?);
        anyhow::bail!("test not implemented")
    }
}
