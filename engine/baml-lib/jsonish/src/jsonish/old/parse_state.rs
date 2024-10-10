use anyhow::Result;
use core::str;
use std::{cell::RefCell, collections::HashMap};

use super::raw_value::{JsonishValue, N};

#[derive(Default)]
pub struct Arena<'a> {
    values: Vec<JsonishValue<'a>>,
}

#[derive(Default, Debug)]
pub(super) struct ParseState<'a> {
    errors: Vec<ParseError<'a>>,
    inner_lookup: HashMap<&'a str, JsonishValue<'a>>,
}

#[derive(Debug)]
enum ErrorType {
    Error(String),
    Warning(String),
}

#[derive(Debug)]
struct ParseError<'a> {
    message: ErrorType,
    location: &'a JsonishValue<'a>,
}

impl<'a> ParseState<'a> {
    fn new() -> Self {
        ParseState {
            errors: Vec::new(),
            inner_lookup: HashMap::new(),
        }
    }

    pub fn record_string(&'a mut self, raw: &'a str, value: String) -> &'a JsonishValue<'a> {
        self.inner_lookup
            .insert(raw, JsonishValue::String(raw, value));
        self.inner_lookup.get(raw).unwrap()
    }

    pub fn record_bool(&'a mut self, raw: &'a str, value: bool) -> &'a JsonishValue<'a> {
        self.inner_lookup
            .insert(raw, JsonishValue::Bool(raw, value));
        self.inner_lookup.get(raw).unwrap()
    }

    pub fn record_number(&'a mut self, raw: &'a str, value: N) -> &'a JsonishValue<'a> {
        self.inner_lookup
            .insert(raw, JsonishValue::Number(raw, value));
        self.inner_lookup.get(raw).unwrap()
    }

    pub fn record_null(&'a mut self, raw: &'a str) -> &'a JsonishValue<'a> {
        self.inner_lookup.insert(raw, JsonishValue::Null(raw));
        self.inner_lookup.get(raw).unwrap()
    }

    pub fn record_ish(&'a mut self, raw: &'a str) -> &'a JsonishValue<'a> {
        self.inner_lookup.insert(raw, JsonishValue::Stringish(raw));
        self.inner_lookup.get(raw).unwrap()
    }

    pub fn record_array(
        &'a mut self,
        raw: &'a str,
        value: Vec<&'a JsonishValue<'a>>,
    ) -> &'a JsonishValue<'a> {
        self.inner_lookup
            .insert(raw, JsonishValue::Array(raw, value));
        self.inner_lookup.get(raw).unwrap()
    }

    pub fn record_object(
        &'a mut self,
        raw: &'a str,
        value: Vec<(&'a JsonishValue<'a>, &'a JsonishValue<'a>)>,
    ) -> &'a JsonishValue<'a> {
        self.inner_lookup
            .insert(raw, JsonishValue::Object(raw, value));
        self.inner_lookup.get(raw).unwrap()
    }

    // This is recursive, and can be very slow for deeply nested structures.
    // But it guarantees that all ish values are resolved.
    pub fn resolve_ish_value(&'a mut self, value: &'a JsonishValue<'a>) -> &'a JsonishValue<'a> {
        if !value.is_ish() {
            return value;
        }

        match self.inner_lookup.get(value.raw()) {
            Some(value) => value,
            None => match value {
                JsonishValue::Stringish(raw) => match JsonishValue::from_json_str(raw, self) {
                    Ok(value) => value,
                    Err(_) => self.record_string(raw, raw.to_string()),
                },
                _ => unreachable!("Error in deserializer. Reached unreachable state"),
            },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn as_error(&self) -> String {
        let mut error_message = String::new();
        for error in &self.errors {
            match &error.message {
                ErrorType::Error(message) => {
                    error_message.push_str("Error: ");
                    error_message.push_str(message);
                }
                ErrorType::Warning(message) => {
                    error_message.push_str("Warning: ");
                    error_message.push_str(message);
                }
            }
            error_message.push_str(" at ");
            error_message.push_str(error.location.raw());
            error_message.push('\n');
        }
        error_message
    }

    pub fn to_error(&self) -> Result<()> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            let mut error_message = String::new();
            for error in &self.errors {
                match &error.message {
                    ErrorType::Error(message) => {
                        error_message.push_str("Error: ");
                        error_message.push_str(message);
                    }
                    ErrorType::Warning(message) => {
                        error_message.push_str("Warning: ");
                        error_message.push_str(message);
                    }
                }
                error_message.push_str(" at ");
                error_message.push_str(error.location.raw());
                error_message.push('\n');
            }
            Err(anyhow::anyhow!(error_message))
        }
    }

    fn new_error<T: Into<String>>(&mut self, location: &'a JsonishValue, message: T) {
        self.errors.push(ParseError {
            message: ErrorType::Error(message.into()),
            location,
        });
    }

    fn new_warning<T: Into<String>>(&mut self, location: &'a JsonishValue, message: T) {
        self.errors.push(ParseError {
            message: ErrorType::Warning(message.into()),
            location,
        });
    }

    pub fn new_not_parsed(&mut self, location: &'a JsonishValue, expected: &str) {
        self.errors.push(ParseError {
            message: ErrorType::Error(format!("Not parseable: Expected: {}", expected)),
            location,
        });
    }

    pub fn new_excess_fields(&mut self, location: &'a JsonishValue, excess_fields: &str) {
        self.errors.push(ParseError {
            message: ErrorType::Warning(format!("Excess field ignored: {}", excess_fields)),
            location,
        });
    }

    pub fn new_missing_fields(&mut self, location: &'a JsonishValue, missing_fields: &str) {
        self.errors.push(ParseError {
            message: ErrorType::Error(format!("Missing required fields: {}", missing_fields)),
            location,
        });
    }

    pub fn new_precision_loss(&mut self, location: &'a JsonishValue, reason: &str) {
        self.errors.push(ParseError {
            message: ErrorType::Warning(format!("Dropped precision: {}", reason)),
            location,
        });
    }

    pub fn new_hallucination(&mut self, location: &'a JsonishValue, expected: &str) {
        self.errors.push(ParseError {
            message: ErrorType::Warning(format!("Hallucination: Expected: {}", expected)),
            location,
        });
    }
}
