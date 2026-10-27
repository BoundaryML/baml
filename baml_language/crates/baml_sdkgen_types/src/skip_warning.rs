use crate::Name;

/// A symbol a generator left out of the SDK because the target language
/// cannot represent its shape yet. Callers surface each one, e.g. as the
/// CLI's ``warning: skipped `user.Foo`: <reason>``.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipWarning {
    /// Fully qualified BAML name of the skipped symbol.
    pub fqn: String,
    /// Whether the symbol is in the project's own package
    /// ([`Name::is_local`]) rather than a dependency such as the standard
    /// library. The CLI lists these one per line and summarizes the rest.
    pub local: bool,
    /// Human-readable reason, e.g. `unsupported type: media`.
    pub reason: String,
}

impl SkipWarning {
    /// The symbol `name` was skipped.
    pub fn new(name: &Name, reason: impl Into<String>) -> Self {
        Self {
            fqn: name.to_string(),
            local: name.is_local(),
            reason: reason.into(),
        }
    }

    /// The method `member` of the class `owner` was skipped.
    pub fn member(owner: &Name, member: &str, reason: impl Into<String>) -> Self {
        Self {
            fqn: format!("{owner}.{member}"),
            local: owner.is_local(),
            reason: reason.into(),
        }
    }
}
