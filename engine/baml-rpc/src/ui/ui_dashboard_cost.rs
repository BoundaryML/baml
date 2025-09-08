use baml_ids::ProjectId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{base::EpochMsTimestamp, rpc::ApiEndpoint};

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetCostStatsRequest {
    #[ts(type = "string")]
    pub project_id: ProjectId,
    #[ts(type = "FilterExpression<number>", optional)]
    pub start_time:
        Option<super::ui_function_calls::FilterExpression<crate::base::EpochMsTimestamp>>,
    #[ts(type = "FilterExpression<number>", optional)]
    pub end_time: Option<super::ui_function_calls::FilterExpression<crate::base::EpochMsTimestamp>>,
    #[ts(optional)]
    pub relative_time: Option<super::ui_function_calls::RelativeTime>,
    #[ts(optional)]
    pub function_name: Option<super::ui_function_calls::FilterExpression<String>>,
    #[ts(optional)]
    pub status: Option<
        super::ui_function_calls::FilterExpression<super::ui_function_calls::FunctionCallStatus>,
    >,
    #[ts(optional)]
    pub tags: Option<Vec<super::ui_function_calls::TagFilter>>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimeSeriesFloatPoint {
    #[ts(type = "number")]
    pub interval_start: EpochMsTimestamp,
    pub value: f64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MetricSeriesFloat {
    pub total: f64,
    pub series: Vec<TimeSeriesFloatPoint>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CostByClientBreakdownItem {
    pub client_name: String,
    pub total_cost: f64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetCostStatsResponse {
    pub total_cost: MetricSeriesFloat,
    pub cost_by_client: Vec<CostByClientBreakdownItem>,
}

pub struct GetCostStats;

impl ApiEndpoint for GetCostStats {
    type Request<'a> = GetCostStatsRequest;
    type Response<'a> = GetCostStatsResponse;

    const PATH: &'static str = "/v1/dashboard/cost";
}
