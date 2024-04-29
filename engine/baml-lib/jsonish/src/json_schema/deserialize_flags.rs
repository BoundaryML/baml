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
            writeln!(
                f,
                "{}",
                serde_json::to_string_pretty(value).unwrap_or(value.to_string())
            )?;
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
    ObjectToField(serde_json::Value),
    StrippedNonAlphaNumeric(String),
    SubstringMatch(String),
    // Values here are the ones ignored
    FirstMatch(Vec<serde_json::Value>),

    NullButHadValue(serde_json::Value),

    // String -> X convertions.
    StringToBool(String),
    StringToNull(String),
    StringToChar(String),

    // Number -> X convertions.
    FloatToInt(f64),

    // X -> Object convertions.
    NoFields(Option<serde_json::Value>),
}

#[derive(Clone)]
pub struct DeserializerConditions {
    flags: Vec<Flag>,
}

impl std::fmt::Debug for DeserializerConditions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for DeserializerConditions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.flags.is_empty() {
            return Ok(());
        }

        writeln!(f, "----Parsing Conditions----")?;
        for flag in &self.flags {
            writeln!(f, "{}", flag)?;
        }
        writeln!(f, "--------------------------")?;
        Ok(())
    }
}

impl std::fmt::Display for Flag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Flag::NullButHadUnparseableValue(ctx, value) => {
                write!(f, "Null but had unparseable value: {}", ctx)?;
                writeln!(f, "----RAW----")?;
                writeln!(
                    f,
                    "{}",
                    serde_json::to_string_pretty(value).unwrap_or(value.to_string())
                )?;
                writeln!(f, "-----------")?;
            }
            Flag::ObjectToString(value) => {
                write!(f, "Object to string: ")?;
                writeln!(f, "{:#?}", value)?;
            }
            Flag::ObjectToField(value) => {
                write!(f, "Object to field: ")?;
                writeln!(f, "{:#?}", value)?;
            }
            Flag::StrippedNonAlphaNumeric(value) => {
                write!(f, "Stripped non-alphanumeric characters: {}", value)?;
            }
            Flag::SubstringMatch(value) => {
                write!(f, "Substring match: {}", value)?;
            }
            Flag::FirstMatch(values) => {
                write!(f, "First match: ")?;
                for value in values {
                    writeln!(f, "{:#?}", value)?;
                }
            }
            Flag::NullButHadValue(value) => {
                write!(f, "Null but had value: ")?;
                writeln!(f, "{:#?}", value)?;
            }
            Flag::StringToBool(value) => {
                write!(f, "String to bool: {}", value)?;
            }
            Flag::StringToNull(value) => {
                write!(f, "String to null: {}", value)?;
            }
            Flag::StringToChar(value) => {
                write!(f, "String to char: {}", value)?;
            }
            Flag::FloatToInt(value) => {
                write!(f, "Float to int: {}", value)?;
            }
            Flag::NoFields(value) => {
                write!(f, "No fields: ")?;
                if let Some(value) = value {
                    writeln!(f, "{:#?}", value)?;
                } else {
                    writeln!(f, "<empty>")?;
                }
            }
        }
        Ok(())
    }
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
