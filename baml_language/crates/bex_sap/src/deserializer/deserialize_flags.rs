use std::borrow::Cow;

use super::{coercer::ParsingError, types::BamlValueWithFlags};
use crate::sap_model::TypeIdent;

/// ## Lifetimes
/// `'s`: the lifetime of the input string
/// `'v`: the lifetime of the parsed [`crate::jsonish::Value`]
/// `'t`: the lifetime of the type being parsed
#[derive(Clone, Debug)]
pub enum Flag<'s, 'v, 't, N: TypeIdent>
where
    's: 'v,
{
    // SingleFromMultiple,
    ObjectFromMarkdown(i32),
    ObjectFromFixedJson(Vec<crate::jsonish::Fixes>),

    DefaultButHadUnparseableValue(ParsingError),
    ObjectToString(Cow<'v, crate::jsonish::Value<'s>>),
    /// When we were expecting a primitive but got a single-field object so used the first field as the value.
    /// If combined with a `Default*` flag, it means the unused json value was an object (not that the default value was an object).
    ObjectToPrimitive(Cow<'v, crate::jsonish::Value<'s>>),
    ObjectToMap(Cow<'v, crate::jsonish::Value<'s>>),
    ExtraKey(Cow<'s, str>, Cow<'v, crate::jsonish::Value<'s>>),
    StrippedNonAlphaNumeric(Cow<'s, str>),
    SubstringMatch(Cow<'s, str>),
    SingleToArray,
    ArrayItemParseError(usize, ParsingError),
    MapKeyParseError(usize, ParsingError),
    MapValueParseError(Cow<'s, str>, ParsingError),
    /// When an optional field's value is present but parsing failed.
    /// The field will be set to its default as if it were missing and this flag will be added to the class object.
    ///
    /// (In the case that a required field is errored, it is an error on the parent object so no flag is added.)
    OptionalFieldError(Cow<'s, str>, ParsingError),

    JsonToString(Cow<'v, crate::jsonish::Value<'s>>),

    /// This key was not present and was inferred from the input (e.g. type was class but input was an array).
    ImpliedKey(Cow<'t, str>),
    InferedObject(Cow<'v, crate::jsonish::Value<'s>>),

    // Values here are all the possible matches.
    FirstMatch(
        usize,
        Vec<Result<BamlValueWithFlags<'s, 'v, 't, N>, ParsingError>>,
    ),
    UnionMatch(
        usize,
        Vec<Result<BamlValueWithFlags<'s, 'v, 't, N>, ParsingError>>,
    ),

    /// `[(value, count)]`
    StrMatchOneFromMany(Vec<(Cow<'t, str>, usize)>),

    /// When a field is missing (in complete objects) or not yet started (in incomplete objects)
    /// and has been filled with a default value.
    ///
    /// The value used is either [`crate::sap_model::AnnotatedField::class_in_progress_field_missing`]
    /// or [`crate::sap_model::AnnotatedField::class_completed_field_missing`].
    DefaultFromNoValue,
    /// When a value is incomplete and the [`crate::sap_model::TypeAnnotations::in_progress`] is set.
    /// The type of `in_progress` should match the expected type.
    ///
    /// Includes the partial value that was present in the input.
    /// Implies [`Flag::Incomplete`].
    DefaultFromInProgress(Cow<'v, crate::jsonish::Value<'s>>),
    DefaultButHadValue(Cow<'v, crate::jsonish::Value<'s>>),
    OptionalDefaultFromNoValue,

    /// `int` value was converted from a parsed string value
    ///
    /// If combined with a `Default*` flag, it means the unused json value was a string (not that the default value was a string).
    StringToInt(Cow<'s, str>),
    /// `bigint` value was converted from a parsed string value.
    ///
    /// If combined with a `Default*` flag, it means the unused json value was a string (not that the default value was a string).
    StringToBigint(Cow<'s, str>),
    /// `bool` value was converted from a parsed string value
    ///
    /// If combined with a `Default*` flag, it means the unused json value was a string (not that the default value was a string).
    StringToBool(Cow<'s, str>),
    /// `null` value was converted from a parsed string value
    ///
    /// If combined with a `Default*` flag, it means the unused json value was a string (not that the default value was a string).
    StringToNull(Cow<'s, str>),
    /// char value was converted from a parsed string value
    ///
    /// If combined with a `Default*` flag, it means the unused json value was a string (not that the default value was a string).
    StringToChar(Cow<'s, str>),
    /// `float` value was converted from a parsed string value
    ///
    /// If combined with a `Default*` flag, it means the unused json value was a string (not that the default value was a string).
    StringToFloat(Cow<'s, str>),

    /// `int` value was converted from a parsed non-integer number
    FloatToInt(f64),

    /// `bigint` value was converted from a parsed non-integer (float) number.
    FloatToBigint(f64),

    // X -> Object conversions.
    NoFields(Option<Cow<'v, crate::jsonish::Value<'s>>>),

    /// Completion state for the top-level node of the value is Incomplete.
    Incomplete,
    Pending,
}

/// A set of flags that describe the conditions under which a value was produced.
#[derive(Clone)]
pub struct DeserializerConditions<'s, 'v, 't, N: TypeIdent>
where
    's: 'v,
{
    pub flags: Vec<Flag<'s, 'v, 't, N>>,
}

impl<N: TypeIdent> std::fmt::Debug for DeserializerConditions<'_, '_, '_, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl<N: TypeIdent> std::fmt::Display for DeserializerConditions<'_, '_, '_, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.flags.is_empty() {
            return Ok(());
        }

        writeln!(f, "----Parsing Conditions----")?;
        for flag in &self.flags {
            writeln!(f, "{flag}")?;
        }
        writeln!(f, "--------------------------")?;
        Ok(())
    }
}

impl<N: TypeIdent> std::fmt::Display for Flag<'_, '_, '_, N> {
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
            Flag::DefaultFromInProgress(value) => {
                write!(f, "Default value from in_progress: {}", value.r#type())?;
            }
            Flag::ObjectFromFixedJson(fixes) => {
                write!(f, "JSON (Fixed {} mistakes)", fixes.len())?;
            }
            Flag::ObjectFromMarkdown(_) => {
                write!(f, "Object from markdown")?;
            }
            Flag::ImpliedKey(key) => {
                write!(f, "Implied key: {key}")?;
            }
            Flag::JsonToString(value) => {
                write!(f, "Json to string: ")?;
                writeln!(f, "{value:#?}")?;
            }
            Flag::ArrayItemParseError(idx, error) => {
                write!(f, "Error parsing item {idx}: {error}")?;
            }
            Flag::MapKeyParseError(idx, error) => {
                write!(f, "Error parsing map key {idx}: {error}")?;
            }
            Flag::MapValueParseError(key, error) => {
                write!(f, "Error parsing map value for key {key}: {error}")?;
            }
            Flag::OptionalFieldError(key, error) => {
                write!(f, "Error parsing optional field {key}: {error}")?;
            }
            Flag::SingleToArray => {
                write!(f, "Converted a single value to an array")?;
            }
            Flag::ExtraKey(key, value) => {
                write!(f, "Extra key: {key}")?;
                writeln!(f, "----RAW----")?;
                writeln!(f, "{value:#?}")?;
                writeln!(f, "-----------")?;
            }
            Flag::StrMatchOneFromMany(values) => {
                write!(f, "Enum one from many: ")?;
                for (value, count) in values {
                    writeln!(f, "Item {value}: {count:?}")?;
                }
            }
            Flag::DefaultButHadUnparseableValue(value) => {
                write!(f, "Null but had unparseable value")?;
                writeln!(f, "----RAW----")?;
                writeln!(f, "{value}")?;
                writeln!(f, "-----------")?;
            }
            Flag::ObjectToString(value) => {
                write!(f, "Object to string: ")?;
                writeln!(f, "{value:#?}")?;
            }
            Flag::ObjectToPrimitive(value) => {
                write!(f, "Object to field: ")?;
                writeln!(f, "{value:#?}")?;
            }
            Flag::ObjectToMap(value) => {
                write!(f, "Object to map: ")?;
                writeln!(f, "{value:#?}")?;
            }
            Flag::StrippedNonAlphaNumeric(value) => {
                write!(f, "Stripped non-alphanumeric characters: {value}")?;
            }
            Flag::SubstringMatch(value) => {
                write!(f, "Substring match: {value}")?;
            }
            Flag::FirstMatch(idx, values) => {
                writeln!(f, "Picked item {idx}:")?;
                for (idx, value) in values.iter().enumerate() {
                    if let Ok(value) = value {
                        writeln!(f, "{idx}: {value:#?}")?;
                    }
                }
            }
            Flag::UnionMatch(idx, values) => {
                writeln!(f, "Picked item {idx}:")?;
                for (idx, value) in values.iter().enumerate() {
                    if let Ok(value) = value {
                        writeln!(f, "{idx}: {value:#?}")?;
                    }
                }
            }
            Flag::DefaultButHadValue(value) => {
                write!(f, "Null but had value: ")?;
                writeln!(f, "{value:#?}")?;
            }
            Flag::StringToInt(value) => {
                write!(f, "String to int: {value}")?;
            }
            Flag::StringToBigint(value) => {
                write!(f, "String to bigint: {value}")?;
            }
            Flag::StringToBool(value) => {
                write!(f, "String to bool: {value}")?;
            }
            Flag::StringToNull(value) => {
                write!(f, "String to null: {value}")?;
            }
            Flag::StringToChar(value) => {
                write!(f, "String to char: {value}")?;
            }
            Flag::StringToFloat(value) => {
                write!(f, "String to float: {value}")?;
            }
            Flag::FloatToInt(value) => {
                write!(f, "Float to int: {value}")?;
            }
            Flag::FloatToBigint(value) => {
                write!(f, "Float to bigint: {value}")?;
            }
            Flag::NoFields(value) => {
                write!(f, "No fields: ")?;
                if let Some(value) = value {
                    writeln!(f, "{value:#?}")?;
                } else {
                    writeln!(f, "<empty>")?;
                }
            }
            Flag::Incomplete => {
                write!(f, "Value is incompletely streamed")?;
            }
            Flag::Pending => {
                write!(f, "Value not yet started")?;
            }
        }
        Ok(())
    }
}

impl<'s, 'v, 't, N: TypeIdent> DeserializerConditions<'s, 'v, 't, N> {
    pub fn add_flag(&mut self, flag: Flag<'s, 'v, 't, N>) {
        self.flags.push(flag);
    }

    #[allow(clippy::must_use_candidate)]
    #[must_use]
    pub fn with_flag(mut self, flag: Flag<'s, 'v, 't, N>) -> Self {
        self.flags.push(flag);
        self
    }

    pub fn new() -> Self {
        Self { flags: Vec::new() }
    }

    pub fn flags(&self) -> &[Flag<'s, 'v, 't, N>] {
        &self.flags
    }
}

impl<N: TypeIdent> Default for DeserializerConditions<'_, '_, '_, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'s, 'v, 't, N: TypeIdent> From<Flag<'s, 'v, 't, N>> for DeserializerConditions<'s, 'v, 't, N> {
    fn from(flag: Flag<'s, 'v, 't, N>) -> Self {
        DeserializerConditions::new().with_flag(flag)
    }
}
