use baml_ids::ProjectId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{base::EpochMsTimestamp, rpc::ApiEndpoint};

/**
interface Expression<T> {
    operator: "gt" | "lt" | "lte" | "gte" | "eq" | "ne" | "in" | "contains" | "regex" | "exists";
    unary_operator?: "length"
    value: T;
}

interface DashboardQueryParams {
  filter?: {
    personId?: string;
    apiKey?: string;
    provider?: string, // openai
    clientId?: string; // client##MyFallbackClient##{interface_hash}##{impl_hash}
    clientName?: string; // MyFallbackClient -> 4o // o1
    functionId?: string; // function##MyFunction##{hash}..
    functionName?: string; // MyFunction
    functionCallId?: string; // specific to uuid / cuid etc
    sessionId?: string;
    callType?: "async" | "sync";
    startAt?: string; // ISO8601
    endAt?: string;   // ISO8601
    relativeTime?: number(minutes|hours|days|weeks|months|years); // e.x. 7d for 7 days
    streamed?: boolean;
    status?: Expression<"success" | "error" | "running">
    input?: {
      [flatJsonPath: string]: Expression<any> // flatJsonPath = company.name
    };
    output?: {
      [flatJsonPath: string]: Expression<any> // company[0].name = Amazon
    };
    tags?: {
     [tagKey: string]: Expression<number, boolean, string>
    };
  };
  graphs?: {
    [graphName: string]: {
      query: "cost" | "counts" | "latency_ms" | "errors"
      groupBy?: | "personId" | "sessionId"
                          | "clientId" | "functionId"
                          | "clientName" | "functionName"
                          | "call_type";
      format: "number" | "percentage";
      time_tick: "day" | "hour" | "week";
      metric: "sum" | "average" | "p50" | "p90" | "p95" | "p99" | "min" | "max";
    };
  };
  //comparison?: "current" | "previous" | "week-over-week" | "month-over-month" | "year-over-year";
}
 */

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum FilterOperation {
    PersonId(UnaryBooleanOperator<String>),
    ApiKeyName(UnaryBooleanOperator<String>),
    Provider(UnaryBooleanOperator<String>),
    ClientId(UnaryBooleanOperator<String>),
    ClientName(UnaryBooleanOperator<String>),
    FunctionId(UnaryBooleanOperator<String>),
    FunctionName(UnaryBooleanOperator<String>),
    SessionId(UnaryBooleanOperator<String>),
    CallType(UnaryBooleanOperator<String>),
    StartAtEpochMs(UnaryBooleanOperator<u64>),
    EndAtEpochMs(UnaryBooleanOperator<u64>),
    RelativeTime(UnaryBooleanOperator<u64>),
    Streamed(UnaryBooleanOperator<bool>),
    Status(UnaryBooleanOperator<String>),
    Input {
        path: String,
        op: UnaryBooleanOperator<String>,
    },
    Output {
        path: String,
        op: UnaryBooleanOperator<String>,
    },
    Tags {
        key: String,
        op: UnaryBooleanOperator<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UnaryBooleanOperator<T> {
    pub operator: String,
    pub value: T,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetDashboardDataRequest {
    #[ts(type = "string")]
    pub project_id: ProjectId,
    #[ts(optional)]
    pub function_id: Option<super::ui_function_calls::FilterExpression<String>>,
    #[ts(optional)]
    pub function_name: Option<super::ui_function_calls::FilterExpression<String>>,
    #[ts(type = "FilterExpression<number>", optional)]
    pub start_time:
        Option<super::ui_function_calls::FilterExpression<crate::base::EpochMsTimestamp>>,
    #[ts(type = "FilterExpression<number>", optional)]
    pub end_time: Option<super::ui_function_calls::FilterExpression<crate::base::EpochMsTimestamp>>,
    #[ts(optional)]
    pub relative_time: Option<super::ui_function_calls::RelativeTime>,
    #[ts(optional)]
    pub tags: Option<Vec<super::ui_function_calls::TagFilter>>,
    #[ts(optional)]
    pub status: Option<
        super::ui_function_calls::FilterExpression<super::ui_function_calls::FunctionCallStatus>,
    >,
    #[ts(optional)]
    pub limit: Option<u32>,
    #[ts(optional)]
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatusCountByFunction {
    pub function_id: String,
    pub success_count: u64,
    pub error_count: u64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ParsingErrorCountByClient {
    pub client_name: String,
    pub error_count: u64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ParsingErrorCountByFunction {
    pub function_id: String,
    pub status_count: u64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatusCountOverTime {
    #[ts(type = "number")]
    pub interval_start: EpochMsTimestamp,
    pub success_count: u64,
    pub error_count: u64,
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
pub struct TimeSeriesIntPoint {
    #[ts(type = "number")]
    pub interval_start: EpochMsTimestamp,
    pub value: u64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MetricSeriesFloat {
    pub total: f64,
    pub series: Vec<TimeSeriesFloatPoint>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MetricSeriesInt {
    pub total: u64,
    pub series: Vec<TimeSeriesIntPoint>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetDashboardDataResponse {
    pub status_counts_by_function: Vec<StatusCountByFunction>,
    pub parsing_errors_by_client: Vec<ParsingErrorCountByClient>,
    pub parsing_errors_by_function: Vec<ParsingErrorCountByFunction>,
    pub status_counts: Vec<StatusCountOverTime>,
    pub latency_p75_ms: MetricSeriesFloat,
    pub latency_p95_ms: MetricSeriesFloat,
    pub latency_avg_ms: MetricSeriesFloat,
    pub total_llm_calls: MetricSeriesInt,
    pub total_traces: MetricSeriesInt,
}

pub struct GetDashboardData;

impl ApiEndpoint for GetDashboardData {
    type Request<'a> = GetDashboardDataRequest;
    type Response<'a> = GetDashboardDataResponse;

    const PATH: &'static str = "/v1/dashboard";
}
