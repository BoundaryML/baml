// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use aws_config::ConfigLoader;
use aws_credential_types::{provider::future::ProvideCredentials, Credentials};
use aws_smithy_async::{
    rt::sleep::{AsyncSleep, Sleep},
    time::TimeSource,
};
use aws_smithy_runtime_api::client::result::{ConnectorError, SdkError};
use aws_smithy_runtime_api::http::{self, Request};
use aws_smithy_runtime_api::{
    client::{
        http::{
            HttpClient, HttpConnector, HttpConnectorFuture, HttpConnectorSettings,
            SharedHttpConnector,
        },
        orchestrator::HttpRequest,
        runtime_components::RuntimeComponents,
    },
    shared::IntoShared,
};
use aws_smithy_types::body::SdkBody;

use aws_config::{BehaviorVersion, SdkConfig};
use chrono::{DateTime, Utc};
use core::pin::Pin;
use core::task::{Context, Poll};
use futures::Stream;
use pin_project_lite::pin_project;
use std::sync::Arc;
use std::time::SystemTime;

use crate::runtime_context::AwsCredProvider;

pub fn load_aws_config() -> ConfigLoader {
    log::debug!("Loading AWS config for wasm specifically");
    let aws_config_contents = r#"
                [profile boundaryml-dev]
sso_session = boundaryml
sso_account_id = 147997132427
sso_role_name = AdministratorAccess
region = us-east-1
output = table
"#;
    aws_config::defaults(BehaviorVersion::latest())
        .sleep_impl(BrowserSleep)
        .time_source(BrowserTime)
        .http_client(BrowserHttp2::new())
        .profile_files(
            aws_runtime::env_config::file::EnvConfigFiles::builder()
                .include_default_config_file(false)
                .include_default_credentials_file(false)
                .with_contents(
                    aws_runtime::env_config::file::EnvConfigFileKind::Config,
                    aws_config_contents,
                )
                .build(),
        )
}

#[derive(Debug)]
struct BrowserTime;
impl TimeSource for BrowserTime {
    fn now(&self) -> SystemTime {
        let offset = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap();
        std::time::UNIX_EPOCH + offset
    }
}

#[derive(Debug, Clone)]
struct BrowserSleep;
impl AsyncSleep for BrowserSleep {
    fn sleep(&self, duration: std::time::Duration) -> Sleep {
        Sleep::new(futures_timer::Delay::new(duration))
    }
}

pin_project! {
    struct StreamWrapper<S> {
        #[pin]
        resp: S,
    }
}

// These are lies, but JsFuture is only !Send because of web workers, so this is
// safe in the web panel: https://github.com/rustwasm/wasm-bindgen/issues/2833
unsafe impl<S> Send for StreamWrapper<S> {}
unsafe impl<S> Sync for StreamWrapper<S> {}

impl<S: Stream<Item = reqwest::Result<bytes::Bytes>>> http_body::Body for StreamWrapper<S> {
    type Data = bytes::Bytes;
    type Error = reqwest::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        let resp = self.project().resp;

        let Poll::Ready(chunk) = resp.poll_next(cx) else {
            return Poll::Pending;
        };
        Poll::Ready(match chunk {
            Some(Ok(chunk_bytes)) => Some(Ok(http_body::Frame::data(chunk_bytes))),
            Some(Err(e)) => Some(Err(e)),
            None => None,
        })
    }
}

#[derive(Debug, Clone)]
struct BrowserHttp2 {
    client: Arc<reqwest::Client>,
}

impl BrowserHttp2 {
    pub fn new() -> Self {
        Self {
            client: Arc::new(reqwest::Client::new()),
        }
    }

    async fn send3(&self, smithy_req: Request) -> Result<http::Response<SdkBody>, ConnectorError> {
        let method = match reqwest::Method::from_bytes(smithy_req.method().as_bytes()) {
            Ok(method) => method,
            Err(e) => return Err(ConnectorError::user(Box::new(e))),
        };
        let mut req = self.client.request(method, smithy_req.uri());

        for (k, v) in smithy_req.headers() {
            req = req.header(k, v);
        }

        if let Some(body) = smithy_req.body().bytes() {
            req = req.body(Vec::from(body));
        }

        match req.send().await {
            Ok(resp) => Ok(http::Response::new(
                resp.status().into(),
                SdkBody::from_body_1_x(StreamWrapper {
                    resp: resp.bytes_stream(),
                }),
            )),
            Err(e) => Err(ConnectorError::other(Box::new(e), None)),
        }
    }
}

impl HttpConnector for BrowserHttp2 {
    fn call(&self, req: HttpRequest) -> HttpConnectorFuture {
        let clone = self.clone();

        HttpConnectorFuture::new(
            async move { send_wrapper::SendWrapper::new(clone.send3(req)).await },
        )
    }
}

impl HttpClient for BrowserHttp2 {
    fn http_connector(
        &self,
        _settings: &HttpConnectorSettings,
        _components: &RuntimeComponents,
    ) -> SharedHttpConnector {
        self.clone().into_shared()
    }
}

pub(super) struct WasmAwsCreds {
    pub default_chain: aws_config::default_provider::credentials::DefaultCredentialsChain,
    // pub aws_cred_provider: Arc<AwsCredProvider>,
}

impl std::fmt::Debug for WasmAwsCreds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmAwsCreds")
            .field("default_chain", &self.default_chain)
            .field("aws_cred_provider", &"<no-repr-available>")
            .finish()
    }
}

impl aws_credential_types::provider::ProvideCredentials for WasmAwsCreds {
    fn provide_credentials<'a>(
        &'a self,
    ) -> aws_credential_types::provider::future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        log::debug!("Providing AWS credentials for wasm");
        // self.default_chain.provide_credentials()
        // let datetime_str = "2025-02-24T19:38:05.000Z";
        // let datetime: DateTime<Utc> = datetime_str.parse().expect("Invalid datetime format");
        // let expires_after =
        //     web_time::UNIX_EPOCH + web_time::Duration::from_secs(datetime.timestamp() as u64);
        ProvideCredentials::ready(Ok(Credentials::new(
            "fake-access-key-id".to_string(),
            "fake-secret-access-key".to_string(),
            None,
            None,
            "hardcoded-boundaryml-dev",
        )))
    }
}
