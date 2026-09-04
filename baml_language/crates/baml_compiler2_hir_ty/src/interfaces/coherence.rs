//! Source spans for the canonical interface coherence report.

use baml_base::Span;
use baml_compiler2_hir::package::PackageId;

/// A reported overlap, mapped to the source ranges used by diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoherenceViolation {
    pub primary: Span,
    pub secondary: Span,
    pub indeterminate: bool,
}

#[salsa::tracked(returns(ref))]
pub fn package_coherence_diagnostics<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
) -> Vec<CoherenceViolation> {
    crate::coherence::package_coherence_violations(db, pkg_id)
        .0
        .iter()
        .map(|violation| CoherenceViolation {
            primary: super::impl_data_source_map(db, violation.primary).impl_span,
            secondary: super::impl_data_source_map(db, violation.secondary).impl_span,
            indeterminate: violation.indeterminate,
        })
        .collect()
}
