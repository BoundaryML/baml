#[derive(Clone)]
pub struct SerializationError {
    scope: Vec<String>,
    message: String,
    value: Option<serde_json::Value>,
}

#[derive(Clone)]
pub struct SerializationContext {
    errors: Vec<SerializationError>,
}

impl std::fmt::Display for SerializationContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for error in &self.errors {
            write!(f, "{}", error)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for SerializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.scope.is_empty() {
            write!(f, "{}: ", self.scope.join("."))?;
        }
        writeln!(f, "{}", self.message)?;
        if let Some(value) = &self.value {
            writeln!(f, "----RAW----")?;
            writeln!(f, "{:#?}", value)?;
            writeln!(f, "-----------")?;
        }
        Ok(())
    }
}

impl SerializationContext {
    pub fn from_error(
        scope: Vec<String>,
        message: String,
        value: Option<serde_json::Value>,
    ) -> Self {
        Self {
            errors: vec![SerializationError {
                scope,
                message,
                value,
            }],
        }
    }
}

#[derive(Clone)]
pub enum Flag {
    NullButHadUnparseableValue(SerializationContext, serde_json::Value),
    ObjectToString(serde_json::Value),
    StrippedNonAlphaNumeric(String),
    SubstringMatch(String),
    // Values here are the ones ignored
    FirstMatch(Vec<serde_json::Value>),
}

#[derive(Clone)]
pub struct DeserializerConditions {
    flags: Vec<Flag>,
}

impl DeserializerConditions {
    pub fn add_flag(&mut self, flag: Flag) {
        self.flags.push(flag);
    }

    pub fn with_flag(mut self, flag: Flag) -> Self {
        self.flags.push(flag);
        self
    }

    pub fn new() -> Self {
        Self { flags: Vec::new() }
    }

    fn score(&self) -> usize {
        self.flags.len()
    }
}

impl Eq for DeserializerConditions {}

impl PartialEq for DeserializerConditions {
    fn eq(&self, other: &Self) -> bool {
        self.score() == other.score()
    }
}

impl PartialOrd for DeserializerConditions {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DeserializerConditions {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score().cmp(&other.score())
    }
}
