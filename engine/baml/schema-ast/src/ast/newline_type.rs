use std::fmt;

/// Defines the newline type of a PSL block.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum NewlineType {
    /// `\\n`
    #[default]
    Unix,
    /// `\\r\\n`
    Windows,
}

impl fmt::Display for NewlineType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl AsRef<str> for NewlineType {
    fn as_ref(&self) -> &str {
        match self {
            NewlineType::Unix => "\n",
            NewlineType::Windows => "\r\n",
        }
    }
}
