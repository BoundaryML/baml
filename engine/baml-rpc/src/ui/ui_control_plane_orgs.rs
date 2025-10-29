use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::rpc::ApiEndpoint;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Organization {
    pub org_id: String,
    pub org_slug: String,
    pub org_display_name: String,
    #[ts(optional)]
    pub stripe_customer_id: Option<String>,
    #[ts(optional)]
    pub stripe_subscription_id: Option<String>,
    #[ts(optional)]
    pub stripe_subscription_status: Option<String>,
    #[ts(optional)]
    pub stripe_entitlements: Option<Vec<String>>,
    /// Complete Stripe subscription state (single source of truth)
    /// Contains subscription data + entitlements from 2 Stripe API calls
    /// stripe_entitlements[] is derived from this JSON for fast indexed queries
    #[ts(optional, type = "any")]
    pub stripe_subscription_data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateOrganizationRequest {
    pub org_id: String,
    pub org_slug: String,
    pub org_display_name: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateOrganizationResponse {
    pub org: Organization,
}

pub struct CreateOrganization;

impl ApiEndpoint for CreateOrganization {
    type Request<'a> = CreateOrganizationRequest;
    type Response<'a> = CreateOrganizationResponse;

    const PATH: &'static str = "/v1/create-organization";
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateOrganizationRequest {
    pub org_id: String,
    #[ts(optional)]
    pub org_slug: Option<String>,
    #[ts(optional)]
    pub org_display_name: Option<String>,
    #[ts(optional)]
    pub stripe_customer_id: Option<String>,
    #[ts(optional)]
    pub stripe_subscription_id: Option<String>,
    #[ts(optional)]
    pub stripe_subscription_status: Option<String>,
    #[ts(optional)]
    pub stripe_entitlements: Option<Vec<String>>,
    #[ts(optional, type = "any")]
    pub stripe_subscription_data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateOrganizationResponse {
    pub org: Organization,
}

pub struct UpdateOrganization;

impl ApiEndpoint for UpdateOrganization {
    type Request<'a> = UpdateOrganizationRequest;
    type Response<'a> = UpdateOrganizationResponse;

    const PATH: &'static str = "/v1/update-organization";
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetOrganizationRequest {
    pub org_id: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetOrganizationResponse {
    pub org: Organization,
}

pub struct GetOrganization;

impl ApiEndpoint for GetOrganization {
    type Request<'a> = GetOrganizationRequest;
    type Response<'a> = GetOrganizationResponse;

    const PATH: &'static str = "/v1/get-organization";
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeleteOrganizationRequest {
    pub org_id: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeleteOrganizationResponse {
    pub success: bool,
}

pub struct DeleteOrganization;

impl ApiEndpoint for DeleteOrganization {
    type Request<'a> = DeleteOrganizationRequest;
    type Response<'a> = DeleteOrganizationResponse;

    const PATH: &'static str = "/v1/delete-organization";
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SyncStripeSubscriptionRequest {
    pub org_id: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SyncStripeSubscriptionResponse {
    pub success: bool,
    #[ts(optional)]
    pub error: Option<String>,
    #[ts(optional, type = "any")]
    pub data: Option<serde_json::Value>,
}

pub struct SyncStripeSubscription;

impl ApiEndpoint for SyncStripeSubscription {
    type Request<'a> = SyncStripeSubscriptionRequest;
    type Response<'a> = SyncStripeSubscriptionResponse;

    const PATH: &'static str = "/v1/billing/sync-stripe-subscription";
}
