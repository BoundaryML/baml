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
