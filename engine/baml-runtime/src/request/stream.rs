use anyhow::Result;
use eventsource_stream::Eventsource;
use futures::stream::StreamExt;
use reqwest::Client;

#[cfg(test)]
pub mod tests {
    use super::*;
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    #[wasm_bindgen_test]
    pub async fn do_test() -> Result<()> {
        wasm_bindgen_test_configure!(run_in_browser);
        console_log::init_with_level(log::Level::Debug)?;
        log::info!("test started");
        let mut stream = reqwest::Client::new()
            .get("http://localhost:7331/events")
            .send()
            .await?
            .bytes_stream()
            .eventsource();

        log::info!("stream created");
        let mut i = 0;
        while let Some(event) = stream.next().await {
            match event {
                Ok(event) => log::info!("received event[type={}]: {}", event.event, event.data),
                Err(e) => log::warn!("error occured: {}", e),
            }
            i += 1;
            if i > 3 {
                break;
            }
        }
        log::info!("stream end");
        anyhow::bail!("test not implemented")
    }
}
