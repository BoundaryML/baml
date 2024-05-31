use internal_baml_diagnostics::Span;

use super::{
    ConfigBlockProperty, Identifier, PrinterConfig, RetryPolicyConfig, WithIdentifier, WithSpan,
};

#[derive(Debug, Clone)]
pub enum Configuration {
    RetryPolicy(RetryPolicyConfig),
    Printer(PrinterConfig),
    TestCase(RetryPolicyConfig),
}
impl Configuration {
    pub fn get_type(&self) -> &'static str {
        match self {
            Configuration::RetryPolicy(_) => "retry_policy",
            Configuration::Printer(_) => "printer",
            Configuration::TestCase(_) => "test",
        }
    }

    pub fn fields(&self) -> &[ConfigBlockProperty] {
        match self {
            Configuration::RetryPolicy(retry_policy) => retry_policy.fields(),
            Configuration::Printer(printer) => printer.fields(),
            Configuration::TestCase(retry_policy) => retry_policy.fields(),
        }
    }
}

impl WithIdentifier for Configuration {
    fn identifier(&self) -> &Identifier {
        match self {
            Configuration::RetryPolicy(retry_policy) => retry_policy.identifier(),
            Configuration::Printer(printer) => printer.identifier(),
            Configuration::TestCase(retry_policy) => retry_policy.identifier(),
        }
    }
}

impl WithSpan for Configuration {
    fn span(&self) -> &Span {
        match self {
            Configuration::RetryPolicy(retry_policy) => retry_policy.span(),
            Configuration::Printer(printer) => printer.span(),
            Configuration::TestCase(retry_policy) => retry_policy.span(),
        }
    }
}
