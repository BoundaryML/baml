//! `baml.errors.*` stdlib error classes this crate raises itself.

use crate::ErrorObject;

/// `baml.errors.InvalidArgument { message }`: an argument is out of range or
/// otherwise invalid. Raised here by the `float` → `int` conversions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidArgument {
    /// Human-readable description, e.g. `"float.itrunc: cannot convert NaN to int"`.
    pub message: String,
}

impl ErrorObject for InvalidArgument {
    fn class_fqn(&self) -> &'static str {
        "baml.errors.InvalidArgument"
    }

    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("message", format!("{:?}", self.message))]
    }
}

/// `baml.errors.ParseError { message }`: text that does not parse as the
/// requested value. Raised here by `bigint.parse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Human-readable description, e.g.
    /// `"bigint.parse: cannot parse \"12a\" as bigint"`.
    pub message: String,
}

impl ErrorObject for ParseError {
    fn class_fqn(&self) -> &'static str {
        "baml.errors.ParseError"
    }

    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("message", format!("{:?}", self.message))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_like_the_vm() {
        let error = InvalidArgument {
            message: "bad \"arg\"".into(),
        };
        assert_eq!(error.class_fqn(), "baml.errors.InvalidArgument");
        assert_eq!(
            error.render_readable(),
            r#"baml.errors.InvalidArgument {message: "bad \"arg\""}"#
        );
        let error = ParseError {
            message: "no".into(),
        };
        assert_eq!(error.class_fqn(), "baml.errors.ParseError");
        assert_eq!(
            error.render_readable(),
            r#"baml.errors.ParseError {message: "no"}"#
        );
    }
}
