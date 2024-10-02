pub use crate::internal::llm_client::LLMResponse;
use crate::{errors::ExposedError, internal::llm_client::orchestrator::OrchestrationScope};
use anyhow::Result;
use colored::*;

use baml_types::BamlValue;
use jsonish::BamlValueWithFlags;

pub struct FunctionResult {
    event_chain: Vec<(
        OrchestrationScope,
        LLMResponse,
        Option<Result<jsonish::BamlValueWithFlags>>,
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
        match &self.parsed() {
            Some(Ok(val)) => {
                let val: BamlValue = val.into();
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
    ) -> Self {
        Self {
            event_chain: vec![(scope, response, parsed)],
        }
    }

    pub(crate) fn event_chain(
        &self,
    ) -> &Vec<(
        OrchestrationScope,
        LLMResponse,
        Option<Result<BamlValueWithFlags>>,
    )> {
        &self.event_chain
    }

    pub fn new_chain(
        chain: Vec<(
            OrchestrationScope,
            LLMResponse,
            Option<Result<BamlValueWithFlags>>,
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

    pub fn parsed_content(&self) -> Result<&BamlValueWithFlags> {
        self.parsed()
            .as_ref()
            .map(|res| {
                if let Ok(val) = res {
                    Ok(val)
                } else {
                    Err(anyhow::anyhow!(ExposedError::ValidationError {
                        prompt: match self.llm_response() {
                            LLMResponse::Success(resp) => resp.prompt.to_string(),
                            LLMResponse::LLMFailure(err) => err.prompt.to_string(),
                            _ => "N/A".to_string(),
                        },
                        raw_response: self
                            .llm_response()
                            .content()
                            .unwrap_or_default()
                            .to_string(),
                        message: match self.llm_response() {
                            LLMResponse::Success(_) =>
                                "Parsing failed for successful LLM response".to_string(),
                            LLMResponse::LLMFailure(err) =>
                                format!("LLM Failure: {} ({})", err.message, err.code.to_string()),
                            LLMResponse::UserFailure(err) => format!("User Failure: {}", err),
                            LLMResponse::InternalFailure(err) =>
                                format!("Internal Failure: {}", err),
                        },
                    }))
                }
            })
            .unwrap_or_else(|| Err(anyhow::anyhow!(self.llm_response().clone())))
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
        if let Some(parsed) = func_res.parsed() {
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
        if self.parsed_content().is_ok() {
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
