//! Per-function throw-analysis facts.
//!
//! These are the *inputs* to the package-level throw-inference solve
//! (`baml_compiler2_hir_ty::throw_facts`): everything extracted from one
//! function's signature and body that the call-graph fixpoint needs. They
//! are a pure function of the defining file's content plus name resolution,
//! which is what makes them safe to persist and re-seed across compiles —
//! the bytecode cache stores them per file in its manifest so an unchanged
//! file's body never needs re-walking just to answer "what does the package
//! throw".
//!
//! Lives in `baml_type` (not the TIR crate) because every layer touches it:
//! TIR extracts and solves, the workspace database carries seeds as a salsa
//! input, and the cache manifest serializes it (hence the borsh derives —
//! `Ty` and `Name` are both borsh-ready).

use std::collections::BTreeSet;

use baml_base::Name;
use borsh::{BorshDeserialize, BorshSerialize};

use crate::Ty;

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FunctionThrowFacts {
    /// Solver key (`throw_set_key` form: namespace-qualified short name).
    pub key: Name,
    /// Direct throw facts: declared `throws` clause (when closed) or facts
    /// collected from the body, exactly as the solver seeds its nodes.
    pub direct: BTreeSet<Ty>,
    /// Same-package call targets (edges of the propagation graph).
    pub call_edges: BTreeSet<Name>,
    /// A closed declared `throws` clause acts as a propagation firewall.
    pub has_declared_contract: bool,
}
