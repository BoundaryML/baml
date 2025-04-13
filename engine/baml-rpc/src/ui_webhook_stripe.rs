use crate::rpc::ApiEndpoint;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StripeWebhookRequest {
    #[ts(type = "any")]
    event: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StripeWebhookResponse {
    received: bool,
}

pub struct StripeWebhook;

impl ApiEndpoint for StripeWebhook {
    type Request = StripeWebhookRequest;
    type Response = StripeWebhookResponse;

    const PATH: &'static str = "/v1/webhooks/stripe";
}
