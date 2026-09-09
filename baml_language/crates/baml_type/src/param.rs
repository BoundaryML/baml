use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::{Name, is_synthetic_effect_param};

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

    /// Whether this is a block-scoped `type T = …` parameter (its index
    /// carries [`SCOPED_PARAM_BIT`]) rather than a declared generic.
    pub fn is_scoped(&self) -> bool {
        self.index & SCOPED_PARAM_BIT != 0
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

/// The generic parameters that survive into one runtime call frame.
///
/// TIR may carry compiler-only parameters for effect inference. Runtime frames
/// omit those parameters and assign dense slots to everything that remains.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeGenericLayout {
    params: Vec<ParamTy>,
}

/// The index bit that marks a block-scoped `type T = …` parameter. Declared
/// generic parameters are De Bruijn positions in a frame and never reach it;
/// a scoped parameter's identity is a hash of its binding statement, and
/// this bit keeps the two spaces disjoint.
pub const SCOPED_PARAM_BIT: u32 = 0x8000_0000;

impl RuntimeGenericLayout {
    pub fn new(params: &[ParamTy]) -> Self {
        Self {
            params: params
                .iter()
                // A synthetic effect parameter (`__effect_param_N`, minted
                // into item signatures) is slot-less by ABI. A block-scoped
                // binding is told apart by its index bit, never by its name,
                // so a user's `type __effect_param_0 = …` keeps its slot.
                .filter(|param| param.is_scoped() || !is_synthetic_effect_param(param.name()))
                .cloned()
                .collect(),
        }
    }

    pub fn params(&self) -> &[ParamTy] {
        &self.params
    }

    pub fn slot(&self, param: &ParamTy) -> Option<u32> {
        self.params
            .iter()
            .position(|candidate| candidate == param)
            .map(Self::slot_index)
    }

    pub fn slot_by_name(&self, name: &Name) -> Option<u32> {
        self.params
            .iter()
            .rposition(|param| param.name() == name)
            .map(Self::slot_index)
    }

    pub fn slots(&self) -> impl Iterator<Item = u32> + '_ {
        (0..self.params.len()).map(Self::slot_index)
    }

    fn slot_index(index: usize) -> u32 {
        u32::try_from(index).expect("runtime type argument index fits in u32")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_index_distinguishes_same_named_parameters() {
        let outer = ParamTy::new(0, Name::new("E"));
        let inner = ParamTy::new(1, Name::new("E"));
        let layout = RuntimeGenericLayout::new(&[outer.clone(), inner.clone()]);

        assert_ne!(outer, inner);
        assert_eq!(layout.slot_by_name(&Name::new("E")), Some(1));
    }

    #[test]
    fn runtime_layout_omits_effect_params_and_compacts_slots() {
        let first = ParamTy::new(0, Name::new("T"));
        let effect = ParamTy::new(1, Name::new("__effect_param_0"));
        let last = ParamTy::new(2, Name::new("U"));
        // A scoped binding is a slot whatever it is called.
        let scoped = ParamTy::new(SCOPED_PARAM_BIT | 5, Name::new("__effect_param_0"));
        let layout = RuntimeGenericLayout::new(&[
            first.clone(),
            effect.clone(),
            last.clone(),
            scoped.clone(),
        ]);

        assert_eq!(
            layout.params(),
            &[first.clone(), last.clone(), scoped.clone()]
        );
        assert_eq!(layout.slot(&first), Some(0));
        assert_eq!(layout.slot(&effect), None);
        assert_eq!(layout.slot(&last), Some(1));
        assert_eq!(layout.slot(&scoped), Some(2));
        assert_eq!(layout.slots().collect::<Vec<_>>(), vec![0, 1, 2]);
    }
}
