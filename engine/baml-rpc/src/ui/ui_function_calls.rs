use crate::base::EpochMsTimestamp;
use crate::rpc::ApiEndpoint;
use crate::ProjectId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use ts_rs::TS;

use super::ui_types;

// TODO: Add support for `in`, `exists`, `contains` operators
#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub enum Operator {
    #[serde(rename = "eq")]
    Eq,
    #[serde(rename = "ne")]
    Ne,
    #[serde(rename = "regex")]
    Regex,
    #[serde(rename = "gt")]
    Gt,
    #[serde(rename = "lt")]
    Lt,
    #[serde(rename = "gte")]
    Gte,
    #[serde(rename = "lte")]
    Lte,
}

impl FromStr for Operator {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "eq" => Ok(Operator::Eq),
            "ne" => Ok(Operator::Ne),
            "regex" => Ok(Operator::Regex),
            "gt" => Ok(Operator::Gt),
            "lt" => Ok(Operator::Lt),
            "gte" => Ok(Operator::Gte),
            "lte" => Ok(Operator::Lte),
            _ => Err(format!("Unknown operator: {}", s)),
        }
    }
}

impl Default for Operator {
    fn default() -> Self {
        Operator::Eq
    }
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct FilterValue<T> {
    pub operator: Operator,
    pub value: T,
}

impl<T> FilterValue<T> {
    pub fn new(operator: Operator, value: T) -> Self {
        Self { operator, value }
    }

    pub fn eq(value: T) -> Self {
        Self::new(Operator::Eq, value)
    }
}

// Query parameters struct for URL deserialization
//
// This struct supports rich filtering through URL query parameters with the following patterns:
//
// Basic filters (defaults to eq operator):
//   ?project_id=proj_123&function_name=myFunction&status=success
//
// Operator-based filters using field__operator format:
//   ?function_name__regex=^test_.*&start_time__gte=1748131389246&status__ne=error
//
// Tag filters:
//   ?tag_environment=production&tag_version__ne=1.0.0&tag_user__regex=admin.*
//
// Supported operators: eq, ne, regex, gt, lt, gte, lte
//
// Examples:
//   - Get calls after a timestamp: ?start_time__gte=1748131389246
//   - Get calls with function name pattern: ?function_name__regex=^extract_.*
//   - Get calls excluding errors: ?status__ne=error
//   - Get calls with specific tag: ?tag_environment=production
//   - Complex query: ?project_id=proj_123&start_time__gte=1748131389246&function_name__regex=^test_.*&tag_env=staging
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ListFunctionCallQueryParams {
    pub project_id: ProjectId,
    pub function_call_id: Option<String>,

    // Simple field filters (defaults to eq operator)
    pub env_id: Option<String>,
    pub person_id: Option<String>,
    pub api_key: Option<String>,
    pub client: Option<String>,
    pub function_id: Option<String>,
    pub function_name: Option<String>,
    pub session_id: Option<String>,
    pub call_type: Option<String>,
    pub call_id: Option<String>,
    pub status: Option<String>,
    pub relative_time: Option<String>,
    pub streamed: Option<bool>,

    // Time filters
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,

    // Operator-based filters (field__operator format)
    pub env_id__eq: Option<String>,
    pub env_id__ne: Option<String>,
    pub env_id__regex: Option<String>,

    pub person_id__eq: Option<String>,
    pub person_id__ne: Option<String>,
    pub person_id__regex: Option<String>,

    pub function_name__eq: Option<String>,
    pub function_name__ne: Option<String>,
    pub function_name__regex: Option<String>,

    pub status__eq: Option<String>,
    pub status__ne: Option<String>,

    pub start_time__gt: Option<u64>,
    pub start_time__gte: Option<u64>,
    pub start_time__lt: Option<u64>,
    pub start_time__lte: Option<u64>,

    pub end_time__gt: Option<u64>,
    pub end_time__gte: Option<u64>,
    pub end_time__lt: Option<u64>,
    pub end_time__lte: Option<u64>,

    // Dynamic tag filters
    #[serde(flatten)]
    pub extra: HashMap<String, String>,
}

impl ListFunctionCallQueryParams {
    /// Convert QueryParams to Filter with proper operator handling
    pub fn to_filter(self) -> Result<Filter, String> {
        let mut filter = Filter::default();

        // Handle simple fields with eq operator or explicit operators
        if let Some(value) = self.env_id {
            filter.env_id = Some(FilterValue::eq(value));
        }
        if let Some(value) = self.env_id__eq {
            filter.env_id = Some(FilterValue::new(Operator::Eq, value));
        }
        if let Some(value) = self.env_id__ne {
            filter.env_id = Some(FilterValue::new(Operator::Ne, value));
        }
        if let Some(value) = self.env_id__regex {
            filter.env_id = Some(FilterValue::new(Operator::Regex, value));
        }

        if let Some(value) = self.person_id {
            filter.person_id = Some(FilterValue::eq(value));
        }
        if let Some(value) = self.person_id__eq {
            filter.person_id = Some(FilterValue::new(Operator::Eq, value));
        }
        if let Some(value) = self.person_id__ne {
            filter.person_id = Some(FilterValue::new(Operator::Ne, value));
        }
        if let Some(value) = self.person_id__regex {
            filter.person_id = Some(FilterValue::new(Operator::Regex, value));
        }

        if let Some(value) = self.api_key {
            filter.api_key = Some(FilterValue::eq(value));
        }
        if let Some(value) = self.client {
            filter.client = Some(FilterValue::eq(value));
        }
        if let Some(value) = self.function_id {
            filter.function_id = Some(FilterValue::eq(value));
        }

        if let Some(value) = self.function_name {
            filter.function_name = Some(FilterValue::eq(value));
        }
        if let Some(value) = self.function_name__eq {
            filter.function_name = Some(FilterValue::new(Operator::Eq, value));
        }
        if let Some(value) = self.function_name__ne {
            filter.function_name = Some(FilterValue::new(Operator::Ne, value));
        }
        if let Some(value) = self.function_name__regex {
            filter.function_name = Some(FilterValue::new(Operator::Regex, value));
        }

        if let Some(value) = self.session_id {
            filter.session_id = Some(FilterValue::eq(value));
        }
        if let Some(value) = self.call_type {
            filter.call_type = Some(FilterValue::eq(value));
        }
        if let Some(value) = self.call_id {
            filter.call_id = Some(FilterValue::eq(value));
        }
        if let Some(value) = self.streamed {
            filter.streamed = Some(FilterValue::eq(value));
        }

        if let Some(value) = self.status {
            filter.status = Some(FilterValue::eq(value));
        }
        if let Some(value) = self.status__eq {
            filter.status = Some(FilterValue::new(Operator::Eq, value));
        }
        if let Some(value) = self.status__ne {
            filter.status = Some(FilterValue::new(Operator::Ne, value));
        }

        if let Some(value) = self.relative_time {
            filter.relative_time = Some(FilterValue::eq(value));
        }

        // Handle time filters
        if let Some(timestamp) = self.start_time {
            let epoch_time = EpochMsTimestamp::try_from(
                web_time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(timestamp),
            )
            .map_err(|e| format!("Invalid start_time timestamp: {}", e))?;
            filter.start_at = Some(FilterValue::eq(epoch_time));
        }

        if let Some(timestamp) = self.start_time__gt {
            let epoch_time = EpochMsTimestamp::try_from(
                web_time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(timestamp),
            )
            .map_err(|e| format!("Invalid start_time__gt timestamp: {}", e))?;
            filter.start_at = Some(FilterValue::new(Operator::Gt, epoch_time));
        }

        if let Some(timestamp) = self.start_time__gte {
            let epoch_time = EpochMsTimestamp::try_from(
                web_time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(timestamp),
            )
            .map_err(|e| format!("Invalid start_time__gte timestamp: {}", e))?;
            filter.start_at = Some(FilterValue::new(Operator::Gte, epoch_time));
        }

        if let Some(timestamp) = self.start_time__lt {
            let epoch_time = EpochMsTimestamp::try_from(
                web_time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(timestamp),
            )
            .map_err(|e| format!("Invalid start_time__lt timestamp: {}", e))?;
            filter.start_at = Some(FilterValue::new(Operator::Lt, epoch_time));
        }

        if let Some(timestamp) = self.start_time__lte {
            let epoch_time = EpochMsTimestamp::try_from(
                web_time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(timestamp),
            )
            .map_err(|e| format!("Invalid start_time__lte timestamp: {}", e))?;
            filter.start_at = Some(FilterValue::new(Operator::Lte, epoch_time));
        }

        if let Some(timestamp) = self.end_time {
            let epoch_time = EpochMsTimestamp::try_from(
                web_time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(timestamp),
            )
            .map_err(|e| format!("Invalid end_time timestamp: {}", e))?;
            filter.end_at = Some(FilterValue::eq(epoch_time));
        }

        if let Some(timestamp) = self.end_time__gt {
            let epoch_time = EpochMsTimestamp::try_from(
                web_time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(timestamp),
            )
            .map_err(|e| format!("Invalid end_time__gt timestamp: {}", e))?;
            filter.end_at = Some(FilterValue::new(Operator::Gt, epoch_time));
        }

        if let Some(timestamp) = self.end_time__gte {
            let epoch_time = EpochMsTimestamp::try_from(
                web_time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(timestamp),
            )
            .map_err(|e| format!("Invalid end_time__gte timestamp: {}", e))?;
            filter.end_at = Some(FilterValue::new(Operator::Gte, epoch_time));
        }

        if let Some(timestamp) = self.end_time__lt {
            let epoch_time = EpochMsTimestamp::try_from(
                web_time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(timestamp),
            )
            .map_err(|e| format!("Invalid end_time__lt timestamp: {}", e))?;
            filter.end_at = Some(FilterValue::new(Operator::Lt, epoch_time));
        }

        if let Some(timestamp) = self.end_time__lte {
            let epoch_time = EpochMsTimestamp::try_from(
                web_time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(timestamp),
            )
            .map_err(|e| format!("Invalid end_time__lte timestamp: {}", e))?;
            filter.end_at = Some(FilterValue::new(Operator::Lte, epoch_time));
        }

        // Handle dynamic tag filters
        let mut tags = HashMap::new();
        for (key, value) in self.extra {
            if let Some(tag_key) = key.strip_prefix("tag_") {
                // Parse tag value - could be JSON or simple string
                let tag_value: serde_json::Value =
                    if value.starts_with('"') || value.starts_with('{') || value.starts_with('[') {
                        serde_json::from_str(&value)
                            .unwrap_or_else(|_| serde_json::Value::String(value))
                    } else {
                        serde_json::Value::String(value)
                    };

                // Check for operator in tag key (e.g., tag_environment__eq)
                if let Some((actual_key, operator_str)) = tag_key.split_once("__") {
                    let operator = Operator::from_str(operator_str).unwrap_or_default();
                    tags.insert(
                        actual_key.to_string(),
                        FilterValue::new(operator, tag_value),
                    );
                } else {
                    tags.insert(tag_key.to_string(), FilterValue::eq(tag_value));
                }
            }
        }

        if !tags.is_empty() {
            filter.tags = Some(tags);
        }

        Ok(filter)
    }
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Filter {
    #[ts(optional)]
    pub env_id: Option<FilterValue<String>>,
    #[ts(optional)]
    pub person_id: Option<FilterValue<String>>,
    #[ts(optional)]
    pub api_key: Option<FilterValue<String>>,
    #[ts(optional)]
    pub client: Option<FilterValue<String>>,
    #[ts(optional)]
    pub function_id: Option<FilterValue<String>>,
    #[ts(optional)]
    pub function_name: Option<FilterValue<String>>,
    #[ts(optional)]
    pub session_id: Option<FilterValue<String>>,
    #[ts(optional)]
    pub call_type: Option<FilterValue<String>>,
    #[ts(type = "FilterValue<number>", optional)]
    pub start_at: Option<FilterValue<EpochMsTimestamp>>,
    #[ts(type = "FilterValue<number>", optional)]
    pub end_at: Option<FilterValue<EpochMsTimestamp>>,
    #[ts(optional)]
    pub relative_time: Option<FilterValue<String>>,
    #[ts(optional)]
    pub call_id: Option<FilterValue<String>>,
    #[ts(optional)]
    pub streamed: Option<FilterValue<bool>>,
    #[ts(optional)]
    pub status: Option<FilterValue<String>>,
    #[ts(type = "Record<string, FilterValue<any>>", optional)]
    pub tags: Option<std::collections::HashMap<String, FilterValue<serde_json::Value>>>,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            env_id: None,
            person_id: None,
            api_key: None,
            client: None,
            function_id: None,
            function_name: None,
            session_id: None,
            call_type: None,
            start_at: None,
            end_at: None,
            relative_time: None,
            call_id: None,
            streamed: None,
            status: None,
            tags: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListFunctionCallsRequest {
    #[ts(type = "string")]
    pub project_id: ProjectId,
    #[ts(optional)]
    pub function_call_id: Option<String>,
    #[ts(optional)]
    pub filter: Option<Filter>,
}

impl ListFunctionCallsRequest {
    /// Create from query parameters
    pub fn from_query_params(params: ListFunctionCallQueryParams) -> Result<Self, String> {
        let project_id = params.project_id.clone();
        let function_call_id = params.function_call_id.clone();
        let filter = Some(params.to_filter()?);

        Ok(Self {
            project_id,
            function_call_id,
            filter,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListFunctionCallsResponse {
    pub function_calls: Vec<ui_types::FunctionCall>,
    // pub function_definitions: Vec<ui_types::FunctionDefinition>,
    // pub type_definitions: Vec<ui_types::TypeDefinition>,
    #[ts(type = "Record<string, any>")]
    pub function_definitions: Vec<serde_json::Value>,
    #[ts(type = "Record<string, any>")]
    pub type_definitions: Vec<serde_json::Value>,
}

pub struct ListFunctionCalls;

impl ApiEndpoint for ListFunctionCalls {
    type Request<'a> = ListFunctionCallsRequest;
    type Response<'a> = ListFunctionCallsResponse;

    const PATH: &'static str = "/v1/function-calls";
}
