use std::collections::HashMap;

use wasm_bindgen::{JsCast, JsValue};

#[derive(Debug)]
pub enum WasmRequestError {
    BuildError(wasm_bindgen::JsValue),
    FetchError(wasm_bindgen::JsValue),
    ResponseError(u16, web_sys::Response),
    JsonError(wasm_bindgen::JsValue),
    SerdeError(wasm_bindgen::JsValue),
}

pub async fn response_text(response: web_sys::Response) -> Result<String, WasmRequestError> {
    let json = wasm_bindgen_futures::JsFuture::from(response.text().map_err(|e| {
        WasmRequestError::JsonError(wasm_bindgen::JsValue::from_str(&format!("{:#?}", e)))
    })?)
    .await
    .map_err(|e| WasmRequestError::JsonError(e))?;

    json.as_string().ok_or(WasmRequestError::JsonError(json))
}

pub async fn response_json<T: serde::de::DeserializeOwned>(
    response: web_sys::Response,
) -> Result<T, WasmRequestError> {
    let json = wasm_bindgen_futures::JsFuture::from(response.json().map_err(|e| {
        WasmRequestError::JsonError(wasm_bindgen::JsValue::from_str(&format!("{:#?}", e)))
    })?)
    .await
    .map_err(|e| WasmRequestError::JsonError(e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| WasmRequestError::SerdeError(e.into()))
}

async fn call_request<T: serde::de::DeserializeOwned>(
    window: web_sys::Window,
    request: &web_sys::Request,
) -> Result<T, WasmRequestError> {
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(request))
        .await
        .map_err(|e| WasmRequestError::FetchError(e))?;

    let resp: web_sys::Response = resp_value
        .dyn_into()
        .map_err(|e| WasmRequestError::FetchError(e))?;

    let status = resp.status();
    if status != 200 {
        return Err(WasmRequestError::ResponseError(status, resp));
    }

    response_json(resp).await
}

pub async fn call_request_with_json<T: serde::de::DeserializeOwned, Body: serde::ser::Serialize>(
    url: &str,
    body: &Body,
    headers: Option<HashMap<String, String>>,
) -> Result<T, WasmRequestError> {
    let window = web_sys::window().unwrap();
    let mut opts = web_sys::RequestInit::new();
    opts.method("POST");
    opts.mode(web_sys::RequestMode::NoCors);
    if let Some(headers) = headers {
        opts.headers(
            &serde_wasm_bindgen::to_value(&headers)
                .map_err(|e| WasmRequestError::BuildError(e.into()))?,
        );
    }

    let body_str = serde_json::to_string(body)
        .map_err(|e| WasmRequestError::BuildError(e.to_string().into()))?;

    opts.body(Some(&JsValue::from_str(&body_str)));

    let request = web_sys::Request::new_with_str_and_init(url, &opts)
        .map_err(|e| WasmRequestError::BuildError(e))?;

    call_request(window, &request).await
}
