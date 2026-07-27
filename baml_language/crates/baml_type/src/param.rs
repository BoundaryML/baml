use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Name;

/// A generic type parameter in its flattened declaration environment.
///
/// `index` is the parameter's slot within its owner's flattened generic frame.
/// `name` is retained for diagnostics and display.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct ParamTy {
    index: u32,
    name: Name,
}

impl ParamTy {
    pub fn new(index: u32, name: Name) -> Self {
        Self { index, name }
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn as_str(&self) -> &str {
        self.name.as_str()
    }

    pub fn extend_frame(frame: &mut Vec<Self>, names: &[Name]) {
        let first_index = u32::try_from(frame.len()).expect("generic parameter count fits in u32");
        frame.extend(names.iter().enumerate().map(|(offset, name)| {
            Self::new(
                first_index + u32::try_from(offset).expect("generic parameter index fits in u32"),
                name.clone(),
            )
        }));
    }
}

impl fmt::Display for ParamTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_index_distinguishes_same_named_parameters() {
        let outer = ParamTy::new(0, Name::new("E"));
        let inner = ParamTy::new(1, Name::new("E"));

        assert_ne!(outer, inner);
    }
}
