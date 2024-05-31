use std::collections::HashMap;

use wasm_bindgen::JsCast;

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
    pass_headers: Option<HashMap<String, String>>,
) -> Result<T, WasmRequestError> {
    let window = web_sys::window().unwrap();
    let mut init = web_sys::RequestInit::new();
    init.method("POST");
    init.mode(web_sys::RequestMode::Cors);
    init.body(Some(&wasm_bindgen::JsValue::from_str(
        &serde_json::to_string(&body).unwrap_or("{}".to_string()),
    )));
    let headers = web_sys::Headers::new().map_err(|e| WasmRequestError::BuildError(e))?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|e| WasmRequestError::BuildError(e))?;
    match pass_headers {
        Some(pass_headers) => {
            for (k, v) in &pass_headers {
                headers
                    .set(k, v)
                    .map_err(|e| WasmRequestError::BuildError(e))?;
            }
        }
        None => {}
    }

    init.headers(&headers);

    let request = web_sys::Request::new_with_str_and_init(url, &init)
        .map_err(|e| WasmRequestError::BuildError(e))?;

    call_request(window, &request).await
}
