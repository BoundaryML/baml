//! The JSON-RPC shapes crossing the WASM boundary.
//!
//! The browser client is `vscode-languageclient/browser` talking over a
//! `MessageChannel`, so messages arrive as plain objects rather than framed
//! bytes; these types are the `tsify` mirror of `lsp_server`'s.

#![expect(
    deprecated,
    reason = "tsify's into_wasm_abi/from_wasm_abi is the browser's established \
              binding for these shapes; moving to `tsify::Ts` changes what the \
              host worker receives, so it is a protocol change (the deprecation \
              is about a wasm-bindgen leak, tracked upstream)"
)]

use baml_lsp::LspError;
use js_sys::Function;
use serde::{Deserialize, Serialize};
use sys_wasm::SendWrapper;
use tsify::Tsify;
use wasm_bindgen::JsValue;

/// Serialize through `serde_json` (NOT `serde-wasm-bindgen`) so
/// `arbitrary_precision` numbers come out as plain JSON numbers instead of the
/// `{ "$serde_json::private::Number": "8" }` struct — which would otherwise
/// leak into every `params` and make every position read as 0 on the JS side.
pub(crate) fn to_json_jsvalue<T: Serialize>(
    value: &T,
    payload_name: &str,
) -> Result<JsValue, LspError> {
    let json = serde_json::to_string(value)
        .map_err(|e| LspError::Internal(format!("failed to serialize {payload_name}: {e}")))?;
    js_sys::JSON::parse(&json).map_err(|e| {
        LspError::Internal(format!("failed to parse serialized {payload_name}: {e:?}"))
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
    pub id: lsp_server::RequestId,
    pub method: String,
    #[tsify(type = "any")]
    pub params: serde_json::Value,
}

#[derive(Tsify, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LspResponse {
    #[tsify(type = "string | number")]
    pub id: lsp_server::RequestId,
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

impl From<LspNotification> for lsp_server::Notification {
    fn from(notification: LspNotification) -> Self {
        Self {
            method: notification.method,
            params: notification.params,
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

/// The host's outbound half: one JS callback for notifications, one for
/// responses. There is no request callback — this server never issues
/// client-bound requests (the native host's equivalent plumbing was dead
/// too), so a `lsp_make_request` the host supplies is simply unused.
pub(crate) struct WasmClientSender {
    send_notification: SendWrapper<Function>,
    send_response: SendWrapper<Function>,
}

impl WasmClientSender {
    pub(crate) fn new(send_notification: Function, send_response: Function) -> Self {
        Self {
            send_notification: SendWrapper::new(send_notification),
            send_response: SendWrapper::new(send_response),
        }
    }

    /// Answer one request. `Err` from the handler becomes the response's
    /// `error` member, exactly as a framed transport would encode it.
    pub(crate) fn respond(
        &self,
        id: lsp_server::RequestId,
        result: Result<serde_json::Value, LspError>,
    ) {
        let response = match result {
            Ok(result) => LspResponse {
                id,
                result: Some(result),
                error: None,
            },
            Err(error) => LspResponse {
                id,
                result: None,
                error: Some(error.to_response_error().into()),
            },
        };
        match to_json_jsvalue(&response, "LSP response") {
            Ok(payload) => {
                if let Err(error) = self.send_response.inner().call1(&JsValue::NULL, &payload) {
                    log::error!("failed to deliver an LSP response to the host: {error:?}");
                }
            }
            Err(error) => log::error!("{error}"),
        }
    }
}

impl baml_lsp::ClientSender for WasmClientSender {
    fn send_notification(&self, method: &str, params: serde_json::Value) -> Result<(), LspError> {
        let notification = LspNotification {
            method: method.to_owned(),
            params,
        };
        let payload = to_json_jsvalue(&notification, "LSP notification")?;
        self.send_notification
            .inner()
            .call1(&JsValue::NULL, &payload)
            .map(|_| ())
            .map_err(|error| {
                LspError::Internal(format!("failed to deliver an LSP notification: {error:?}"))
            })
    }
}
