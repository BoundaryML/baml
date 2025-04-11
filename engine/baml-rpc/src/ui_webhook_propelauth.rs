use crate::rpc::ApiEndpoint;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PropelAuthWebhookRequest {
    // PropelAuth webhooks typically send a JSON payload
    #[ts(type = "any")]
    payload: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PropelAuthWebhookResponse {
    received: bool,
}

pub struct PropelAuthWebhook;

impl ApiEndpoint for PropelAuthWebhook {
    type Request = PropelAuthWebhookRequest;
    type Response = PropelAuthWebhookResponse;

    const PATH: &'static str = "/v1/webhooks/propelauth";
}
