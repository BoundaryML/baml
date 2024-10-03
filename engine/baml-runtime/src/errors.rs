pub enum ExposedError {
    /// Error in parsing post calling the LLM
    ValidationError {
        prompt: String,
        raw_response: String,
        message: String,
    },
}

impl std::error::Error for ExposedError {}

impl std::fmt::Display for ExposedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExposedError::ValidationError {
                prompt,
                raw_response,
                message,
            } => {
                write!(
                    f,
                    "Parsing error: {}\nPrompt: {}\nRaw Response: {}",
                    message, prompt, raw_response
                )
            }
        }
    }
}

impl std::fmt::Debug for ExposedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}
