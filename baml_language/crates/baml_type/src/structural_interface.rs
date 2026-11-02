use borsh::{BorshDeserialize, BorshSerialize};

/// Language interfaces with a shared native implementation when no explicit
/// implementation applies. The declaring interface's identity grants this
/// capability; a method's spelling alone never does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum StructuralInterface {
    ToString,
    ToJson,
    FromJson,
    Equals,
    Hash,
    /// Conditional, like `Hash`: only tuples whose every slot implements
    /// `Compare` qualify, ordering lexicographically through the slots.
    Compare,
}

impl StructuralInterface {
    /// Whether membership needs a per-receiver eligibility proof rather than
    /// holding for every admissible type.
    pub const fn is_conditional(self) -> bool {
        matches!(self, Self::Hash | Self::Compare)
    }

    pub const fn primary_method(self) -> &'static str {
        match self {
            Self::ToString => "to_string",
            Self::ToJson => "to_json",
            Self::FromJson => "from_json",
            Self::Equals => "eq",
            Self::Hash => "hash",
            Self::Compare => "cmp",
        }
    }

    pub const fn native_function(self) -> &'static str {
        match self {
            Self::ToString => "_to_string_default",
            Self::ToJson => "_to_json_default",
            Self::FromJson => "_from_json_structural_default",
            Self::Equals => "_equals_structural_default",
            Self::Hash => "_hash_structural_default",
            Self::Compare => "_compare_structural_default",
        }
    }

    pub const fn native_type_arg_count(self) -> usize {
        match self {
            Self::Equals | Self::Hash | Self::Compare => 0,
            Self::ToString | Self::ToJson | Self::FromJson => 1,
        }
    }
}
