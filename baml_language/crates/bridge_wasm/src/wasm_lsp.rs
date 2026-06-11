use bex_project::LspError;
use js_sys::Function;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsValue;

use crate::send_wrapper::SendWrapper;

/// Serialize a value to a `JsValue` via `serde_json` (NOT serde-wasm-bindgen),
/// so `arbitrary_precision` numbers come out as plain JSON numbers instead of
/// the `{ "$serde_json::private::Number": "8" }` struct. Used for outbound LSP
/// payloads, whose `serde_json::Value` params would otherwise leak that tag and
/// make every position read as 0 on the JS side.
pub(crate) fn to_json_jsvalue<T: Serialize>(
    value: &T,
    payload_name: &str,
) -> Result<JsValue, LspError> {
    let json = serde_json::to_string(value).map_err(|e| {
        LspError::UnknownErrorCode(format!("failed to serialize {payload_name} payload: {e}"))
    })?;
    js_sys::JSON::parse(&json).map_err(|e| {
        LspError::UnknownErrorCode(format!(
            "failed to parse serialized {payload_name} payload as JSON: {e:?}"
        ))
    })
}

#[derive(Tsify, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LspNotification {
    pub method: String,
    #[serde(default = "serde_json::Value::default")]
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    #[tsify(type = "any")]
    pub params: serde_json::Value,
}

#[derive(Tsify, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LspRequest {
    #[tsify(type = "string | number")]
    id: lsp_server::RequestId,
    method: String,
    #[tsify(type = "any")]
    params: serde_json::Value,
}

#[derive(Tsify, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LspResponse {
    #[tsify(type = "string | number")]
    id: lsp_server::RequestId,
    #[tsify(type = "any")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<LspResponseError>,
}

#[derive(Tsify, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LspResponseError {
    pub code: i32,
    pub message: String,
    #[tsify(type = "any")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub data: Option<serde_json::Value>,
}

impl From<LspRequest> for lsp_server::Request {
    fn from(request: LspRequest) -> Self {
        Self {
            id: request.id,
            method: request.method,
            params: request.params,
        }
    }
}

impl From<lsp_server::Request> for LspRequest {
    fn from(request: lsp_server::Request) -> Self {
        Self {
            id: request.id,
            method: request.method,
            params: request.params,
        }
    }
}

impl From<LspNotification> for lsp_server::Notification {
    fn from(notification: LspNotification) -> Self {
        Self {
            method: notification.method,
            params: notification.params,
        }
    }
}

impl From<lsp_server::Notification> for LspNotification {
    fn from(notification: lsp_server::Notification) -> Self {
        Self {
            method: notification.method,
            params: notification.params,
        }
    }
}

impl From<lsp_server::Response> for LspResponse {
    fn from(response: lsp_server::Response) -> Self {
        Self {
            id: response.id,
            result: response.result,
            error: response.error.map(Into::into),
        }
    }
}

impl From<LspResponse> for lsp_server::Response {
    fn from(response: LspResponse) -> Self {
        Self {
            id: response.id,
            result: response.result,
            error: response.error.map(Into::into),
        }
    }
}

impl From<lsp_server::ResponseError> for LspResponseError {
    fn from(error: lsp_server::ResponseError) -> Self {
        Self {
            code: error.code,
            message: error.message,
            data: error.data,
        }
    }
}

impl From<LspResponseError> for lsp_server::ResponseError {
    fn from(error: LspResponseError) -> Self {
        Self {
            code: error.code,
            message: error.message,
            data: error.data,
        }
    }
}

/// WASM env implementation that holds the JS `env_vars` callback.
///
/// Signature of the JS function: `(var: string) => Promise<string | undefined>`
#[allow(clippy::struct_field_names)]
pub(crate) struct WasmLsp {
    /// The JS function to call for env lookups.
    send_notification_fn: SendWrapper<Function>,
    send_response_fn: SendWrapper<Function>,
    make_request_fn: SendWrapper<Function>,
}

impl WasmLsp {
    pub(crate) fn new(
        send_notification_fn: Function,
        send_response_fn: Function,
        make_request_fn: Function,
    ) -> Self {
        Self {
            send_notification_fn: SendWrapper::new(send_notification_fn),
            send_response_fn: SendWrapper::new(send_response_fn),
            make_request_fn: SendWrapper::new(make_request_fn),
        }
    }

    fn send_notification(&self, notification: lsp_server::Notification) -> Result<(), LspError> {
        let send_notification_fn = self.send_notification_fn.inner();
        let notif: LspNotification = notification.into();
        let payload = to_json_jsvalue(&notif, "LSP notification")?;
        send_notification_fn
            .call1(&JsValue::NULL, &payload)
            .map(|_| ())
            .map_err(|e| {
                LspError::UnknownErrorCode(format!("failed to send LSP notification to JS: {e:?}"))
            })
    }

    fn send_response(&self, response: lsp_server::Response) -> Result<(), LspError> {
        let send_response_fn = self.send_response_fn.inner();
        let response: LspResponse = response.into();
        let payload = to_json_jsvalue(&response, "LSP response")?;
        send_response_fn
            .call1(&JsValue::NULL, &payload)
            .map(|_| ())
            .map_err(|e| {
                LspError::UnknownErrorCode(format!("failed to send LSP response to JS: {e:?}"))
            })
    }

    fn make_request(&self, request: lsp_server::Request) -> Result<(), LspError> {
        let make_request_fn = self.make_request_fn.inner();
        let request: LspRequest = request.into();
        let payload = to_json_jsvalue(&request, "LSP request")?;
        make_request_fn
            .call1(&JsValue::NULL, &payload)
            .map(|_| ())
            .map_err(|e| {
                LspError::UnknownErrorCode(format!("failed to send LSP request to JS: {e:?}"))
            })
    }
}

impl bex_project::LspClientSenderTrait for WasmLsp {
    fn send_notification(&self, notif: lsp_server::Notification) -> Result<(), LspError> {
        self.send_notification(notif)
    }

    fn send_response_impl(&self, response: lsp_server::Response) -> Result<(), LspError> {
        self.send_response(response)
    }

    fn make_request(&self, req: lsp_server::Request) -> Result<(), LspError> {
        self.make_request(req)
    }
}
