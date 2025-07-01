use baml_runtime::{
    js_callback_provider::{set_js_callback_provider, GcpCredResult, JsCallbackResult},
    AwsCredResult, JsCallbackProvider, RuntimeCallbackError,
};
use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// A trait for invoking JS callbacks from Rust WASM.
///
/// See `init_js_callback_bridge` for more details.
trait JsCallbackBridge {
    type TCallbackResult: serde::de::DeserializeOwned;
    const CALLBACK_NAME: &'static str;

    async fn invoke(
        callback: &js_sys::Function,
        arg: Option<String>,
    ) -> Result<Self::TCallbackResult, RuntimeCallbackError> {
        let Ok(load) = callback.call1(&JsValue::NULL, &JsValue::from(arg)) else {
            return Err(RuntimeCallbackError::JsCallbackTypeError(format!(
                "{} did not return a promise",
                Self::CALLBACK_NAME
            )));
        };

        let load = match JsFuture::from(Promise::unchecked_from_js(load)).await {
            Err(e) => {
                if let Some(e_as_error) = e.dyn_ref::<js_sys::Error>() {
                    if let Some(e_as_str) = e_as_error.message().as_string() {
                        return Err(RuntimeCallbackError::JsCallbackTypeError(format!(
                            "{} rejected during promise await with {}: {}",
                            Self::CALLBACK_NAME,
                            e_as_error.name(),
                            e_as_str
                        )));
                    }
                }
                return Err(RuntimeCallbackError::JsCallbackTypeError(format!(
                    "{} rejected during promise await: {:?}",
                    Self::CALLBACK_NAME,
                    e
                )));
            }
            Ok(load) => load,
        };

        match serde_wasm_bindgen::from_value::<JsCallbackResult<Self::TCallbackResult>>(load) {
            Ok(retval) => match retval {
                JsCallbackResult::Ok(retval) => Ok(retval),
                JsCallbackResult::Err(e) => Err(RuntimeCallbackError::JsCallbackRuntimeError {
                    name: e.name,
                    message: e.message,
                }),
            },
            Err(e) => Err(RuntimeCallbackError::JsCallbackTypeError(format!(
                "Failed to deserialize {} return value into {}: {:?}",
                Self::CALLBACK_NAME,
                std::any::type_name::<Self::TCallbackResult>(),
                e
            ))),
        }
    }
}

struct AwsCredProvider;

impl JsCallbackBridge for AwsCredProvider {
    type TCallbackResult = AwsCredResult;
    const CALLBACK_NAME: &'static str = "loadAwsCreds";
}

struct GcpCredProvider;

impl JsCallbackBridge for GcpCredProvider {
    type TCallbackResult = GcpCredResult;
    const CALLBACK_NAME: &'static str = "loadGcpCreds";
}

// TODO: trait-ify the loop method (but I think it's actually more boilerplate to trait-ify it than to just copy-paste right now)
async fn loop_aws_cred_provider(
    load_aws_creds_cb: js_sys::Function,
    mut req_rx: tokio::sync::mpsc::Receiver<Option<String>>,
    resp_tx: tokio::sync::broadcast::Sender<Result<AwsCredResult, RuntimeCallbackError>>,
) {
    while let Some(profile_name) = req_rx.recv().await {
        let _ = resp_tx.send(AwsCredProvider::invoke(&load_aws_creds_cb, profile_name).await);
    }
    let _ = resp_tx.send(Err(RuntimeCallbackError::RecvError(
        "request channel closed".to_string(),
    )));
}

async fn loop_gcp_cred_provider(
    load_gcp_creds_cb: js_sys::Function,
    mut req_rx: tokio::sync::mpsc::Receiver<Option<String>>,
    resp_tx: tokio::sync::broadcast::Sender<Result<GcpCredResult, RuntimeCallbackError>>,
) {
    while (req_rx.recv().await).is_some() {
        let _ = resp_tx.send(GcpCredProvider::invoke(&load_gcp_creds_cb, None).await);
    }
    let _ = resp_tx.send(Err(RuntimeCallbackError::RecvError(
        "request channel closed".to_string(),
    )));
}

#[wasm_bindgen]
/// This allows us to invoke JS callbacks from Rust.
///
/// We need to do this as a wildly hacky workaround because (1) wasm in the webview is sandboxed and doesn't have easy
/// access to env vars and (2) js_sys::Value is not Send which causes a bunch of painful issues with tokio, since
/// the compiler-generated futures need to be Send even though we don't use web workers.
pub fn init_js_callback_bridge(
    load_aws_creds_cb: js_sys::Function,
    load_gcp_creds_cb: js_sys::Function,
) {
    let (aws_req_tx, aws_req_rx) = tokio::sync::mpsc::channel::<Option<String>>(100);
    let (aws_resp_tx, aws_resp_rx) =
        tokio::sync::broadcast::channel::<Result<AwsCredResult, RuntimeCallbackError>>(100);
    let (gcp_req_tx, gcp_req_rx) = tokio::sync::mpsc::channel::<Option<String>>(100);
    let (gcp_resp_tx, gcp_resp_rx) =
        tokio::sync::broadcast::channel::<Result<GcpCredResult, RuntimeCallbackError>>(100);

    set_js_callback_provider(JsCallbackProvider::new(
        aws_req_tx,
        aws_resp_rx,
        gcp_req_tx,
        gcp_resp_rx,
    ));
    wasm_bindgen_futures::spawn_local(loop_aws_cred_provider(
        load_aws_creds_cb,
        aws_req_rx,
        aws_resp_tx,
    ));
    wasm_bindgen_futures::spawn_local(loop_gcp_cred_provider(
        load_gcp_creds_cb,
        gcp_req_rx,
        gcp_resp_tx,
    ));
}
