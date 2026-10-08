//! `baml.errors.*` stdlib error classes this crate raises itself.

use crate::{ErrorObject, render_object};

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

    fn render_readable(&self) -> String {
        render_object(
            self.class_fqn(),
            &[("message", format!("{:?}", self.message))],
        )
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
    }
}
