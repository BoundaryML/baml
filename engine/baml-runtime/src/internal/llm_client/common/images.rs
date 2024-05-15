use anyhow::Result;
use base64::prelude::*;
use mime_guess::MimeGuess;

#[cfg(feature = "no_wasm")]
async fn fetch_image(url: &str) -> Result<Vec<u8>> {
    use reqwest;
    let response = reqwest::get(url).await?;
    let image_data = response.bytes().await?.to_vec();
    Ok(image_data)
}

#[cfg(not(feature = "no_wasm"))]
async fn fetch_image(url: &str) -> Result<Vec<u8>> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);

    let request =
        Request::new_with_str_and_init(url, &opts).map_err(|e| anyhow::anyhow!("{:#?}", e))?;

    let window = web_sys::window().unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch request: {:#?}", e))?;

    let resp: Response = resp_value.dyn_into().unwrap();
    let buf = JsFuture::from(
        resp.array_buffer()
            .map_err(|e| anyhow::anyhow!("{:#?}", e))?,
    )
    .await
    .map_err(|e| anyhow::anyhow!("{:#?}", e))?;
    let array = js_sys::Uint8Array::new(&buf);
    Ok(array.to_vec())
}

pub async fn download_image_as_base64(url: &str) -> Result<(String, String)> {
    let guessed_mime_type = MimeGuess::from_path(url)
        .first_or_octet_stream()
        .to_string();

    let image_data = fetch_image(url).await?;

    let encoded_image = BASE64_STANDARD.encode(&image_data);

    Ok((guessed_mime_type, encoded_image))
}
