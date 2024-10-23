pub use crate::internal::llm_client::LLMResponse;
use crate::{errors::ExposedError, internal::llm_client::{orchestrator::OrchestrationScope, ResponseBamlValue}};
use anyhow::Result;
use colored::*;

use baml_types::BamlValue;
use jsonish::BamlValueWithFlags;

#[derive(Debug)]
pub struct FunctionResult {
    event_chain: Vec<(
        OrchestrationScope,
        LLMResponse,
        Option<Result<BamlValueWithFlags>>,
        Option<Result<ResponseBamlValue>>,
    )>,
}

impl std::fmt::Display for FunctionResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // print out the number of previous tries only if there was more than 1
        if self.event_chain.len() > 1 {
            writeln!(
                f,
                "{}",
                format!("({} other previous tries)", self.event_chain.len() - 1).yellow()
            )?;
        }
        writeln!(f, "{}", self.llm_response())?;
        match &self.result_with_constraints() {
            Some(Ok(val)) => {
                writeln!(
                    f,
                    "{}",
                    format!("---Parsed Response ({})---", val.r#type()).blue()
                )?;
                write!(f, "{:#}", serde_json::json!(val))
            }
            Some(Err(e)) => {
                writeln!(f, "{}", "---Parsed Response---".blue())?;
                write!(f, "{}", e.to_string().red())
            }
            None => Ok(()),
        }
    }
}

impl FunctionResult {
    pub fn new(
        scope: OrchestrationScope,
        response: LLMResponse,
        parsed: Option<Result<BamlValueWithFlags>>,
        baml_value: Option<Result<ResponseBamlValue>>,
    ) -> Self {
        Self {
            event_chain: vec![(scope, response, parsed, baml_value)],
        }
    }

    pub(crate) fn event_chain(
        &self,
    ) -> &Vec<(
        OrchestrationScope,
        LLMResponse,
        Option<Result<BamlValueWithFlags>>,
        Option<Result<ResponseBamlValue>>,
    )> {
        &self.event_chain
    }

    pub fn new_chain(
        chain: Vec<(
            OrchestrationScope,
            LLMResponse,
            Option<Result<BamlValueWithFlags>>,
            Option<Result<ResponseBamlValue>>,
        )>,
    ) -> Result<Self> {
        if chain.is_empty() {
            anyhow::bail!("No events in the chain");
        }

        Ok(Self { event_chain: chain })
    }

    pub fn content(&self) -> Result<&str> {
        self.llm_response().content()
    }

    pub fn llm_response(&self) -> &LLMResponse {
        &self.event_chain.last().unwrap().1
    }

    pub fn scope(&self) -> &OrchestrationScope {
        &self.event_chain.last().unwrap().0
    }

    pub fn parsed(&self) -> &Option<Result<BamlValueWithFlags>> {
        &self.event_chain.last().unwrap().2
    }

    /// Get the parsed result. This logic is strange because parsing errors can
    /// be forwarded to a different field in the orchestrator.
    /// TODO: (Greg) Fix the strange logic.
    /// Historical note: Most of the consumers of the orchestrator use a final
    /// `ResponseBamlValue`, a type designed to hold only the information needed
    /// in those responses. But one consumer, the wasm client, requires extra info
    /// from the parsing stage. Therefore we preserve both the parsing stage data
    /// and the `ResponseValue` side by side. And because `anyhow::Error` is not
    /// `Clone`, errors from the parsing stage are handled the most easily by
    /// migrating them to the `ResponseValue` in cases where parsing failed.
    /// The proper solution is to create a `RuntimeBamlValue` that contains
    /// enough information for all clients, and then types like
    /// `SDKClientResponseBamlValue` and `WasmResponseBamlValue` which derive
    /// from `RuntimeBamlValue` where needed.
    pub fn parsed_content(&self) -> Result<&BamlValueWithFlags> {
        match (self.parsed(), self.result_with_constraints()) {
            // Error at parse time was forwarded to later result.
            (None, Some(Err(e))) => Err(self.format_err(e)),
            // Parsing succeeded.
            (Some(Ok(v)), _) => Ok(v),
            // Error at parse time was not forwarded to later results.
            (Some(Err(e)), _) => Err(self.format_err(e)),
            (None, None) => Err(anyhow::anyhow!(self.llm_response().clone())),
            (None, Some(_)) => unreachable!("A response could not have been created without a successful parse")
        }
    }

    pub fn result_with_constraints(&self) -> &Option<Result<ResponseBamlValue>> {
        &self.event_chain.last().unwrap().3
    }

    pub fn result_with_constraints_content(&self) -> Result<&ResponseBamlValue> {
        self.result_with_constraints()
            .as_ref()
            .map(|res| {
                if let Ok(val) = res {
                    Ok(val)
                } else {
                    Err(self.format_err( res.as_ref().err().unwrap() ))
                }
            })
            .unwrap_or_else(|| Err(anyhow::anyhow!(self.llm_response().clone())))
    }

    fn format_err(&self, err: &anyhow::Error) -> anyhow::Error {
        // Capture the actual error to preserve its details
        let actual_error = err.to_string();
        anyhow::anyhow!(ExposedError::ValidationError {
            prompt: match self.llm_response() {
                LLMResponse::Success(resp) => resp.prompt.to_string(),
                LLMResponse::LLMFailure(err) => err.prompt.to_string(),
                _ => "N/A".to_string(),
            },
            raw_output: self
                .llm_response()
                .content()
                .unwrap_or_default()
                .to_string(),
            // The only branch that should be hit is LLMResponse::Success(_) since we
            // only call this function when we have a successful response.
            message: match self.llm_response() {
                LLMResponse::Success(_) =>
                    format!("Failed to parse LLM response: {}", actual_error),
                LLMResponse::LLMFailure(err) => format!(
                    "LLM Failure: {} ({}) - {}",
                    err.message,
                    err.code.to_string(),
                    actual_error
                ),
                LLMResponse::UserFailure(err) =>
                    format!("User Failure: {} - {}", err, actual_error),
                LLMResponse::InternalFailure(err) =>
                    format!("Internal Failure: {} - {}", err, actual_error),
            },
        })
    }
}

pub struct TestResponse {
    pub function_response: FunctionResult,
    pub function_span: Option<uuid::Uuid>,
}

impl std::fmt::Display for TestResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.function_response)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TestStatus<'a> {
    Pass,
    Fail(TestFailReason<'a>),
}

impl From<TestStatus<'_>> for BamlValue {
    fn from(status: TestStatus) -> Self {
        match status {
            TestStatus::Pass => BamlValue::String("pass".to_string()),
            TestStatus::Fail(r) => BamlValue::String(format!("failed! {:?}", r)),
        }
    }
}

#[derive(Debug)]
pub enum TestFailReason<'a> {
    TestUnspecified(&'a anyhow::Error),
    TestLLMFailure(&'a LLMResponse),
    TestParseFailure(&'a anyhow::Error),
}

impl PartialEq for TestFailReason<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::TestUnspecified(a), Self::TestUnspecified(b)) => a.to_string() == b.to_string(),
            (Self::TestLLMFailure(_), Self::TestLLMFailure(_)) => true,
            (Self::TestParseFailure(a), Self::TestParseFailure(b)) => {
                a.to_string() == b.to_string()
            }
            _ => false,
        }
    }
}

impl Eq for TestFailReason<'_> {}

impl TestResponse {
    pub fn status(&self) -> TestStatus {
        let func_res = &self.function_response;
        if let Some(parsed) = func_res.result_with_constraints() {
            if parsed.is_ok() {
                TestStatus::Pass
            } else {
                TestStatus::Fail(TestFailReason::TestParseFailure(
                    parsed.as_ref().unwrap_err(),
                ))
            }
        } else {
            TestStatus::Fail(TestFailReason::TestLLMFailure(func_res.llm_response()))
        }
    }
}

#[cfg(test)]
use std::process::Termination;

// This allows tests to pass or fail based on the contents of the FunctionResult
#[cfg(test)]
impl Termination for FunctionResult {
    fn report(self) -> std::process::ExitCode {
        if self.result_with_constraints_content().is_ok() {
            std::process::ExitCode::SUCCESS
        } else {
            std::process::ExitCode::FAILURE
        }
    }
}

// This allows tests to pass or fail based on the contents of the TestResponse
#[cfg(test)]
impl Termination for TestResponse {
    fn report(self) -> std::process::ExitCode {
        if self.status() == TestStatus::Pass {
            std::process::ExitCode::SUCCESS
        } else {
            std::process::ExitCode::FAILURE
        }
    }
}
