//! The value slots of a call, independent of the values' types and of who
//! executes the call.
//!
//! A call site pushes one value per slot of the signature it was checked
//! against; a callee reads one value per slot of its own declaration. The two
//! agree on the required slots and on the *names* of optional slots, but an
//! implementation may declare optionals the caller's signature never mentions
//! (an interface impl with an extra defaulted parameter, a function value
//! narrowed to fewer optionals). Dispatch reconciles them with [`CallLayout::map_to`].
use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::{FunctionParamMode, Name};

/// One entry per value slot: `None` is a required positional slot, `Some(name)`
/// an optional named slot. An omitted optional still occupies its slot (as the
/// omission sentinel), so `len()` is exactly the number of values pushed.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CallLayout(pub Vec<Option<Name>>);

/// Why a caller's slots cannot be laid over a target's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutMismatch {
    /// The target declares more required slots than the caller pushed.
    MissingRequired { expected: usize, got: usize },
    /// The caller pushed more required slots than the target declares.
    ExtraRequired { expected: usize, got: usize },
    /// The caller supplied an optional the target does not declare.
    UnknownOptional(Name),
}

impl fmt::Display for LayoutMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequired { expected, got } | Self::ExtraRequired { expected, got } => {
                write!(f, "expected {expected} required arguments, got {got}")
            }
            Self::UnknownOptional(name) => {
                write!(f, "call supplies optional parameter `{name}` the callee does not declare")
            }
        }
    }
}

impl std::error::Error for LayoutMismatch {}

impl CallLayout {
    /// `count` required slots and nothing else.
    pub fn positional(count: usize) -> Self {
        Self(vec![None; count])
    }

    /// The layout a declared parameter list reads. Optional parameters are
    /// always named in a declaration.
    pub fn from_modes(params: impl IntoIterator<Item = (Option<Name>, FunctionParamMode)>) -> Self {
        Self(
            params
                .into_iter()
                .map(|(name, mode)| match mode {
                    FunctionParamMode::Required => None,
                    FunctionParamMode::Optional => {
                        Some(name.expect("optional parameter has a name"))
                    }
                })
                .collect(),
        )
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn required_count(&self) -> usize {
        self.0.iter().filter(|slot| slot.is_none()).count()
    }

    /// For each of `target`'s slots, the index of the caller slot that fills
    /// it, or `None` for an optional the caller never mentioned (the callee
    /// receives its omission sentinel there). Required slots pair up in
    /// order; optional slots pair up by name. The whole map is validated
    /// before any value moves, so a mismatch leaves the caller's values intact.
    pub fn map_to(&self, target: &Self) -> Result<Vec<Option<usize>>, LayoutMismatch> {
        let (got, expected) = (self.required_count(), target.required_count());
        if got < expected {
            return Err(LayoutMismatch::MissingRequired { expected, got });
        }
        if got > expected {
            return Err(LayoutMismatch::ExtraRequired { expected, got });
        }
        if let Some(unknown) = self
            .0
            .iter()
            .flatten()
            .find(|name| !target.0.contains(&Some((*name).clone())))
        {
            return Err(LayoutMismatch::UnknownOptional(unknown.clone()));
        }
        let mut required = self
            .0
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.is_none())
            .map(|(index, _)| index);
        Ok(target
            .0
            .iter()
            .map(|slot| match slot {
                None => required.next(),
                Some(name) => self.0.iter().position(|caller| caller.as_ref() == Some(name)),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(slots: &[Option<&str>]) -> CallLayout {
        CallLayout(slots.iter().map(|slot| slot.map(Name::new)).collect())
    }

    #[test]
    fn reorders_named_slots_and_omits_implementation_extras() {
        let caller = layout(&[None, Some("prefix"), Some("suffix")]);
        let target = layout(&[None, Some("suffix"), Some("extra"), Some("prefix")]);
        assert_eq!(
            caller.map_to(&target).unwrap(),
            vec![Some(0), Some(2), None, Some(1)]
        );
        assert_eq!(
            CallLayout::positional(1).map_to(&target).unwrap(),
            vec![Some(0), None, None, None]
        );
        assert_eq!(target.map_to(&target).unwrap(), vec![Some(0), Some(1), Some(2), Some(3)]);
    }

    #[test]
    fn invalid_shapes_fail_before_mapping_values() {
        assert_eq!(
            CallLayout::positional(2).map_to(&CallLayout::positional(1)),
            Err(LayoutMismatch::ExtraRequired {
                expected: 1,
                got: 2
            })
        );
        assert_eq!(
            CallLayout::positional(1).map_to(&CallLayout::positional(2)),
            Err(LayoutMismatch::MissingRequired {
                expected: 2,
                got: 1
            })
        );
        assert_eq!(
            layout(&[Some("a")]).map_to(&layout(&[Some("b")])),
            Err(LayoutMismatch::UnknownOptional(Name::new("a")))
        );
    }
}
