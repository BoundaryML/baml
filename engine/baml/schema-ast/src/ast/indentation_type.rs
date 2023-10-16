use std::fmt;

/// Defines the indentation of a PSL block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IndentationType {
    /// Uses a tab character.
    Tabs,
    /// Uses the given amount of spaces.
    Spaces(usize),
}

impl Default for IndentationType {
    /// Prisma defaults to the JavaScript default of two spaces.
    fn default() -> Self {
        Self::Spaces(2)
    }
}

impl fmt::Display for IndentationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tabs => f.write_str("\t"),
            Self::Spaces(num) => {
                for _ in 0..*num {
                    f.write_str(" ")?;
                }

                Ok(())
            }
        }
    }
}
