use baml_runtime::{
    remote_cred_provider::{set_remote_cred_provider, GcpCredResult},
    AwsCredProviderImpl, AwsCredResult, RuntimeCallbackError,
};
use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

async fn invoke_aws_cred_provider(
    load_aws_creds_cb: &js_sys::Function,
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

async fn invoke_gcp_cred_provider(
    load_gcp_creds_cb: &js_sys::Function,
) -> Result<GcpCredResult, RuntimeCallbackError> {
    let Ok(load) = load_gcp_creds_cb.call0(&JsValue::NULL) else {
        return Err(RuntimeCallbackError::AwsCredProviderError(
            "loadGcpCreds did not return a promise".to_string(),
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

    let creds_result = match serde_wasm_bindgen::from_value::<GcpCredResult>(load) {
        Ok(creds) => Ok(creds),
        Err(e) => Err(RuntimeCallbackError::AwsCredProviderError(format!(
            "Expected loadGcpCreds to return an GcpCredResult. {}",
            e
        ))),
    };

    creds_result
}

async fn loop_aws_cred_provider(
    load_aws_creds_cb: js_sys::Function,
    mut req_rx: tokio::sync::mpsc::Receiver<Option<String>>,
    resp_tx: tokio::sync::broadcast::Sender<Result<AwsCredResult, RuntimeCallbackError>>,
) {
    while let Some(profile_name) = req_rx.recv().await {
        let _ = resp_tx.send(invoke_aws_cred_provider(&load_aws_creds_cb, profile_name).await);
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
    while let Some(_) = req_rx.recv().await {
        let _ = resp_tx.send(invoke_gcp_cred_provider(&load_gcp_creds_cb).await);
    }
    let _ = resp_tx.send(Err(RuntimeCallbackError::RecvError(
        "request channel closed".to_string(),
    )));
}

#[wasm_bindgen]
pub fn init_aws_cred_provider(
    load_aws_creds_cb: js_sys::Function,
    load_gcp_creds_cb: js_sys::Function,
) {
    let (aws_req_tx, aws_req_rx) = tokio::sync::mpsc::channel::<Option<String>>(100);
    let (aws_resp_tx, aws_resp_rx) =
        tokio::sync::broadcast::channel::<Result<AwsCredResult, RuntimeCallbackError>>(100);
    let (gcp_req_tx, gcp_req_rx) = tokio::sync::mpsc::channel::<Option<String>>(100);
    let (gcp_resp_tx, gcp_resp_rx) =
        tokio::sync::broadcast::channel::<Result<GcpCredResult, RuntimeCallbackError>>(100);

    set_remote_cred_provider(AwsCredProviderImpl::new(
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
