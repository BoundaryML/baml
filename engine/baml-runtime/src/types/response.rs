pub use crate::internal::llm_client::LLMResponse;
use anyhow::Result;

#[derive(Debug)]
pub struct FunctionResult {
    pub(crate) llm_response: LLMResponse,
    pub(crate) parsed: Option<Result<(serde_json::Value, jsonish::DeserializerConditions)>>,
}

impl FunctionResult {
    pub fn content(&self) -> Result<&str> {
        self.llm_response.content()
    }

    pub fn parsed_content(&self) -> Result<&serde_json::Value> {
        self.parsed
            .as_ref()
            .map(|res| {
                if let Ok((val, _)) = res {
                    Ok(val)
                } else {
                    anyhow::bail!("{:#?}", self)
                }
            })
            .unwrap_or_else(|| anyhow::bail!("{:#?}", self))
    }
}

pub struct TestResponse {
    pub(crate) function_response: Result<FunctionResult>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TestStatus<'a> {
    Pass,
    Fail(TestFailReason<'a>),
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
        match &self.function_response {
            Ok(func_res) => {
                if let Some(parsed) = &func_res.parsed {
                    if parsed.is_ok() {
                        TestStatus::Pass
                    } else {
                        TestStatus::Fail(TestFailReason::TestParseFailure(
                            parsed.as_ref().unwrap_err(),
                        ))
                    }
                } else {
                    TestStatus::Fail(TestFailReason::TestLLMFailure(&func_res.llm_response))
                }
            }
            Err(e) => TestStatus::Fail(TestFailReason::TestUnspecified(e)),
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
