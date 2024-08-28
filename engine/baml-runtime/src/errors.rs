pub enum ExposedError {
    /// Error in parsing post calling the LLM
    ValidationError(String),
}

impl std::error::Error for ExposedError {}

impl std::fmt::Display for ExposedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExposedError::ValidationError(err) => {
                write!(f, "Parsing error: {}", err)
            }
        }
    }
}

impl std::fmt::Debug for ExposedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}
