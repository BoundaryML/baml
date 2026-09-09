//! Default bridge representation of a class declaration, chosen by its owner.

/// This is independent of recursive portability: a copied record can contain
/// live children. Interface-typed positions always obtain checked live views.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize,
)]
pub enum ClassProjection {
    Record,
    Live,
    /// A compiler-owned carrier has a dedicated codec (media, prompt, etc.).
    /// Its methods must not replace that codec with the authored-class rule.
    Builtin,
}
