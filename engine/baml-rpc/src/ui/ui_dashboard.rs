use crate::base::EpochMsTimestamp;
use crate::rpc::ApiEndpoint;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/**
 * interface Expression<T> {
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
pub enum ChartMetric {
    TotalTokenCount,
    LatencyMs,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChartParameters {
    pub chart_name: String,
    pub chart_metric: ChartMetric,
    pub group_by: Vec<String>,
    pub filters: Vec<FilterOperation>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChartDefinition {
    pub chart_name: String,
    pub chart_params: ChartParameters,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetDashboardDataRequest {
    pub charts: Vec<ChartDefinition>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChartDataSeries {
    pub series_name: String,
    /// If there are no data points for a given time window, the value will be None.
    /// The frontend can decide whether to show a gap or a zero.
    #[ts(type = "[number, number | null][]")]
    pub data: Vec<(EpochMsTimestamp, Option<f64>)>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChartData {
    pub chart_name: String,
    pub chart_params: ChartParameters,
    /// There is one data series per member of the group_by set,
    /// e.g. if group_by = ["personId", "functionId"], there will be one
    /// data series per (personId, functionId) pair.
    pub chart_data: Vec<ChartDataSeries>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetDashboardDataResponse {
    pub charts: Vec<ChartData>,
}

struct GetDashboardData;

impl ApiEndpoint for GetDashboardData {
    type Request<'a> = GetDashboardDataRequest;
    type Response<'a> = GetDashboardDataResponse;

    const PATH: &'static str = "/v1/dashboard";
}
