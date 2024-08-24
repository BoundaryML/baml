use super::{coercer::ParsingError, types::BamlValueWithFlags};

#[derive(Debug, Clone)]
pub enum Flag {
    // SingleFromMultiple,
    ObjectFromMarkdown(i32),
    ObjectFromFixedJson(Vec<crate::jsonish::Fixes>),

    DefaultButHadUnparseableValue(ParsingError),
    ObjectToString(crate::jsonish::Value),
    ObjectToPrimitive(crate::jsonish::Value),
    ObjectToMap(crate::jsonish::Value),
    ExtraKey(String, crate::jsonish::Value),
    StrippedNonAlphaNumeric(String),
    SubstringMatch(String),
    SingleToArray,
    ArrayItemParseError(usize, ParsingError),
    MapKeyParseError(usize, ParsingError),
    MapValueParseError(String, ParsingError),

    JsonToString(crate::jsonish::Value),
    ImpliedKey(String),
    InferedObject(crate::jsonish::Value),

    // Values here are all the possible matches.
    FirstMatch(usize, Vec<Result<BamlValueWithFlags, ParsingError>>),
    UnionMatch(usize, Vec<Result<BamlValueWithFlags, ParsingError>>),

    EnumOneFromMany(Vec<(usize, String)>),

    DefaultFromNoValue,
    DefaultButHadValue(crate::jsonish::Value),
    OptionalDefaultFromNoValue,

    // String -> X convertions.
    StringToBool(String),
    StringToNull(String),
    StringToChar(String),

    // Number -> X convertions.
    FloatToInt(f64),

    // X -> Object convertions.
    NoFields(Option<crate::jsonish::Value>),
}

#[derive(Clone)]
pub struct DeserializerConditions {
    pub(super) flags: Vec<Flag>,
}

impl DeserializerConditions {
    pub fn explanation(&self) -> Option<String> {
        let flags = self
            .flags
            .iter()
            .filter_map(|c| match c {
                // Flag::ObjectFromMarkdown(_) => None,
                // Flag::ObjectFromFixedJson(_) => Some(format!("Object from fixed JSON")),
                // Flag::ArrayItemParseError(idx, e) => {
                //     Some(format!("Error parsing item {} in array: {}", idx, e))
                // }
                // Flag::DefaultButHadUnparseableValue(e) => {
                //     Some(format!("Default but had unparseable value {e}"))
                // }
                // Flag::ObjectToString(_) => Some(format!("Object to string")),
                // Flag::ObjectToPrimitive(_) => Some(format!("Object to primitive")),
                // Flag::ObjectToMap(_) => Some(format!("Object to map")),
                // Flag::ExtraKey(_, _) => None,
                // Flag::StrippedNonAlphaNumeric(_) => Some(format!("Stripped non-alphanumeric")),
                // Flag::SubstringMatch(_) => Some(format!("Substring match")),
                // Flag::SingleToArray => Some(format!("Single to array")),
                // Flag::MapKeyParseError(idx, e) => {
                //     Some(format!("Error parsing key {} in map: {}", idx, e))
                // }
                // Flag::MapValueParseError(key, e) => Some(format!(
                //     "Error parsing value for key '{}' in map: {}",
                //     key, e
                // )),
                // Flag::JsonToString(_) => Some(format!("JSON to string")),
                // Flag::ImpliedKey(_) => Some(format!("Implied key")),
                // Flag::InferedObject(_) => Some(format!("Inferred object")),
                // Flag::FirstMatch(idx, _) => Some(format!("First match at index {}", idx)),
                // Flag::EnumOneFromMany(matches) => {
                //     Some(format!("Enum one from many with {} matches", matches.len()))
                // }
                // Flag::DefaultFromNoValue => Some(format!("Default from no value")),
                // Flag::DefaultButHadValue(_) => Some(format!("Default but had value")),
                // Flag::OptionalDefaultFromNoValue => Some(format!("Optional default from no value")),
                // Flag::StringToBool(_) => Some(format!("String to bool")),
                // Flag::StringToNull(_) => Some(format!("String to null")),
                // Flag::StringToChar(_) => Some(format!("String to char")),
                // Flag::FloatToInt(_) => Some(format!("Float to int")),
                // Flag::NoFields(_) => Some(format!("No fields")),
                Flag::ObjectFromMarkdown(_) => None,
                Flag::ObjectFromFixedJson(_) => None,
                Flag::ArrayItemParseError(idx, e) => {
                    Some(format!("Error parsing item {} in array: {}", idx, e))
                }
                Flag::DefaultButHadUnparseableValue(e) => {
                    Some(format!("Default but had unparseable value {e}"))
                }
                Flag::ObjectToString(_) => None,
                Flag::ObjectToPrimitive(_) => None,
                Flag::ObjectToMap(_) => None,
                Flag::ExtraKey(_, _) => None,
                Flag::StrippedNonAlphaNumeric(_) => None,
                Flag::SubstringMatch(_) => None,
                Flag::SingleToArray => None,
                Flag::MapKeyParseError(idx, e) => {
                    Some(format!("Error parsing key {} in map: {}", idx, e))
                }
                Flag::MapValueParseError(key, e) => Some(format!(
                    "Error parsing value for key '{}' in map: {}",
                    key, e
                )),
                Flag::JsonToString(_) => None,
                Flag::ImpliedKey(_) => None,
                Flag::InferedObject(_) => None,
                Flag::FirstMatch(_idx, _) => None,
                Flag::EnumOneFromMany(_matches) => None,
                Flag::DefaultFromNoValue => None,
                Flag::DefaultButHadValue(_) => None,
                Flag::OptionalDefaultFromNoValue => None,
                Flag::StringToBool(_) => None,
                Flag::StringToNull(_) => None,
                Flag::StringToChar(_) => None,
                Flag::FloatToInt(_) => None,
                Flag::NoFields(_) => None,
            })
            .map(|s| format!("<flag>{}</flag>", s))
            .collect::<Vec<_>>();

        match flags.len() {
            0 => None,
            _ => Some(flags.join("; ")),
        }
    }
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
            Flag::InferedObject(value) => {
                write!(f, "Infered object from: {}", value.r#type())?;
            }
            Flag::OptionalDefaultFromNoValue => {
                write!(f, "Optional Default value")?;
            }
            Flag::DefaultFromNoValue => {
                write!(f, "Default value")?;
            }
            Flag::ObjectFromFixedJson(fixes) => {
                write!(f, "JSON (Fixed {} mistakes)", fixes.len())?;
            }
            Flag::ObjectFromMarkdown(_) => {
                write!(f, "Object from markdown")?;
            }
            Flag::ImpliedKey(key) => {
                write!(f, "Implied key: {}", key)?;
            }
            Flag::JsonToString(value) => {
                write!(f, "Json to string: ")?;
                writeln!(f, "{:#?}", value)?;
            }
            Flag::ArrayItemParseError(idx, error) => {
                write!(f, "Error parsing item {}: {}", idx, error)?;
            }
            Flag::MapKeyParseError(idx, error) => {
                write!(f, "Error parsing map key {}: {}", idx, error)?;
            }
            Flag::MapValueParseError(key, error) => {
                write!(f, "Error parsing map value for key {}: {}", key, error)?;
            }
            Flag::SingleToArray => {
                write!(f, "Converted a single value to an array")?;
            }
            Flag::ExtraKey(key, value) => {
                write!(f, "Extra key: {}", key)?;
                writeln!(f, "----RAW----")?;
                writeln!(f, "{:#?}", value)?;
                writeln!(f, "-----------")?;
            }
            Flag::EnumOneFromMany(values) => {
                write!(f, "Enum one from many: ")?;
                for (idx, value) in values {
                    writeln!(f, "Item {}: {}", idx, value)?;
                }
            }
            Flag::DefaultButHadUnparseableValue(value) => {
                write!(f, "Null but had unparseable value")?;
                writeln!(f, "----RAW----")?;
                writeln!(f, "{}", value)?;
                writeln!(f, "-----------")?;
            }
            Flag::ObjectToString(value) => {
                write!(f, "Object to string: ")?;
                writeln!(f, "{:#?}", value)?;
            }
            Flag::ObjectToPrimitive(value) => {
                write!(f, "Object to field: ")?;
                writeln!(f, "{:#?}", value)?;
            }
            Flag::ObjectToMap(value) => {
                write!(f, "Object to map: ")?;
                writeln!(f, "{:#?}", value)?;
            }
            Flag::StrippedNonAlphaNumeric(value) => {
                write!(f, "Stripped non-alphanumeric characters: {}", value)?;
            }
            Flag::SubstringMatch(value) => {
                write!(f, "Substring match: {}", value)?;
            }
            Flag::FirstMatch(idx, values) => {
                writeln!(f, "Picked item {}:", idx)?;
                for (idx, value) in values.iter().enumerate() {
                    if let Ok(value) = value {
                        writeln!(f, "{idx}: {:#?}", value)?;
                    }
                }
            }
            Flag::UnionMatch(idx, values) => {
                writeln!(f, "Picked item {}:", idx)?;
                for (idx, value) in values.iter().enumerate() {
                    if let Ok(value) = value {
                        writeln!(f, "{idx}: {:#?}", value)?;
                    }
                }
            }
            Flag::DefaultButHadValue(value) => {
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

    pub fn flags(&self) -> &Vec<Flag> {
        &self.flags
    }
}

impl Default for DeserializerConditions {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Flag> for DeserializerConditions {
    fn from(flag: Flag) -> Self {
        DeserializerConditions::new().with_flag(flag)
    }
}
