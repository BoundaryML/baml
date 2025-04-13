use crate::rpc::ApiEndpoint;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Organization {
    pub org_id: String,
    pub org_slug: String,
    pub org_display_name: String,
    pub stripe_customer_id: Option<String>,
    pub stripe_subscription_id: Option<String>,
    pub stripe_subscription_status: Option<String>,
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
    type Request = CreateOrganizationRequest;
    type Response = CreateOrganizationResponse;

    const PATH: &'static str = "/v1/create-organization";
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateOrganizationRequest {
    pub org_id: String,
    pub org_slug: Option<String>,
    pub org_display_name: Option<String>,
    pub stripe_customer_id: Option<String>,
    pub stripe_subscription_id: Option<String>,
    pub stripe_subscription_status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateOrganizationResponse {
    pub org: Organization,
}

pub struct UpdateOrganization;

impl ApiEndpoint for UpdateOrganization {
    type Request = UpdateOrganizationRequest;
    type Response = UpdateOrganizationResponse;

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
    type Request = GetOrganizationRequest;
    type Response = GetOrganizationResponse;

    const PATH: &'static str = "/v1/get-organization";
}
