use baml_runtime::{AwsCredProvider, AwsCredProviderImpl, AwsCredResult, RuntimeCallbackError};
use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

async fn invoke_aws_cred_provider(
    load_aws_creds_cb: js_sys::Function,
    profile_name: Option<String>,
) -> Result<AwsCredResult, RuntimeCallbackError> {
    let Ok(load) = load_aws_creds_cb.call1(&JsValue::NULL, &JsValue::from(profile_name)) else {
        return Err(RuntimeCallbackError::AwsCredProviderError(
            "loadAwsCreds did not return a promise".to_string(),
        ));
    };

    let load = JsFuture::from(Promise::unchecked_from_js(load)).await;

    let load = match load {
        Ok(load) => load,
        Err(err) => {
            if let Some(e) = err.dyn_ref::<js_sys::Error>() {
                if let Some(e_str) = e.message().as_string() {
                    return Err(RuntimeCallbackError::AwsCredProviderError(format!(
                        "loadAwsCreds failure: {}",
                        e_str
                    )));
                }
            }

            return Err(RuntimeCallbackError::AwsCredProviderError(format!(
                "loadAwsCreds rejected: {:?}",
                err
            )));
        }
    };

    let creds_result = match serde_wasm_bindgen::from_value::<AwsCredResult>(load) {
        Ok(creds) => Ok(creds),
        Err(e) => Err(RuntimeCallbackError::AwsCredProviderError(format!(
            "Expected loadAwsCreds to return an AwsCredResult. {}",
            e
        ))),
    };

    creds_result
}

async fn drive_aws_cred_provider(
    load_aws_creds_cb: js_sys::Function,
    mut req_rx: tokio::sync::mpsc::Receiver<Option<String>>,
    resp_tx: tokio::sync::broadcast::Sender<Result<AwsCredResult, RuntimeCallbackError>>,
) {
    let Some(profile_name) = req_rx.recv().await else {
        let _ = resp_tx.send(Err(RuntimeCallbackError::AwsCredProviderError(
            "request channel closed".to_string(),
        )));
        return;
    };

    let _ = resp_tx.send(invoke_aws_cred_provider(load_aws_creds_cb, profile_name).await);
}

pub fn js_fn_to_aws_cred_provider(load_aws_creds_cb: js_sys::Function) -> AwsCredProvider {
    let (req_tx, req_rx) = tokio::sync::mpsc::channel::<Option<String>>(1);
    let (resp_tx, resp_rx) =
        tokio::sync::broadcast::channel::<Result<AwsCredResult, RuntimeCallbackError>>(1);

    wasm_bindgen_futures::spawn_local(drive_aws_cred_provider(load_aws_creds_cb, req_rx, resp_tx));

    Some(AwsCredProviderImpl { req_tx, resp_rx })
}
