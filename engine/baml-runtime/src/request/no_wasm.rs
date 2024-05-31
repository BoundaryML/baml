use std::collections::HashMap;

use anyhow::Error;

#[derive(Debug)]
pub enum NoWasmRequestError {
    BuildError(Error),
    FetchError(Error),
    ResponseError(u16, reqwest::Response),
    JsonError(Error),
    SerdeError(Error),
}

pub async fn response_text(response: reqwest::Response) -> Result<String, NoWasmRequestError> {
    let text = response
        .text()
        .await
        .map_err(|e| NoWasmRequestError::JsonError(e.into()))?;

    Ok(text)
}

pub async fn response_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, NoWasmRequestError> {
    let json = response
        .json()
        .await
        .map_err(|e| NoWasmRequestError::JsonError(e.into()))?;

    serde_json::from_value(json).map_err(|e| NoWasmRequestError::SerdeError(e.into()))
}

pub async fn call_request_with_json<T: serde::de::DeserializeOwned, Body: serde::ser::Serialize>(
    url: &str,
    body: &Body,
    headers: Option<HashMap<String, String>>,
) -> Result<T, NoWasmRequestError> {
    let client = reqwest::Client::new();
    let mut request = client.post(url).json(body);
    if let Some(headers) = headers {
        for (key, value) in headers {
            request = request.header(key, value);
        }
    }

    let response = request
        .send()
        .await
        .map_err(|e| NoWasmRequestError::FetchError(e.into()))?;

    let status = response.status();

    if !status.is_success() {
        return Err(NoWasmRequestError::ResponseError(status.as_u16(), response));
    }

    response_json(response).await
}
