use internal_baml_diagnostics::Span;

use super::{
    ConfigBlockProperty, Identifier, PrinterConfig, RetryPolicyConfig, WithIdentifier, WithSpan,
};

#[derive(Debug, Clone)]
pub enum Configuration {
    RetryPolicy(RetryPolicyConfig),
    Printer(PrinterConfig),
}
impl Configuration {
    pub fn get_type(&self) -> &'static str {
        match self {
            Configuration::RetryPolicy(_) => "retry_policy",
            Configuration::Printer(_) => "printer",
        }
    }

    pub fn fields(&self) -> &[ConfigBlockProperty] {
        match self {
            Configuration::RetryPolicy(retry_policy) => retry_policy.fields(),
            Configuration::Printer(printer) => printer.fields(),
        }
    }
}

impl WithIdentifier for Configuration {
    fn identifier(&self) -> &Identifier {
        match self {
            Configuration::RetryPolicy(retry_policy) => retry_policy.identifier(),
            Configuration::Printer(printer) => printer.identifier(),
        }
    }
}

impl WithSpan for Configuration {
    fn span(&self) -> &Span {
        match self {
            Configuration::RetryPolicy(retry_policy) => retry_policy.span(),
            Configuration::Printer(printer) => printer.span(),
        }
    }
}
