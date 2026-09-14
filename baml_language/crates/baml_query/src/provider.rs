//! Provider contracts and the trusted logical-to-physical boundary.
//!
//! A backend supplies one [`RelationProviderFactory`]; the session wraps
//! every produced table in [`TrustedRelation`], which (1) pins the public
//! schema to the catalog definition and (2) guarantees that no filter
//! containing value expressions is ever pushed into a provider — value
//! predicates are DataFusion-owned residual work by decision (D3/D7).

use std::sync::Arc;

use async_trait::async_trait;
use datafusion::{
    arrow::datatypes::SchemaRef,
    catalog::Session,
    common::Result as DfResult,
    datasource::{TableProvider, TableType},
    logical_expr::{Expr, TableProviderFilterPushDown},
    physical_plan::ExecutionPlan,
};

use crate::{catalog::RelationDef, error::QueryError, scope::Snapshot};

/// How exactly a provider can execute one pushed-down filter.
/// (The `DataFusion` spelling is [`TableProviderFilterPushDown`]; this alias
/// names the contract in design terms.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushdownClass {
    /// The provider applies the predicate with exactly the public
    /// semantics; `DataFusion` will not re-check it.
    Exact,
    /// The provider may reduce rows but `DataFusion` re-checks the original
    /// predicate (a final limit never trusts an inexact filter).
    InexactCandidate,
    /// The provider cannot help; the filter stays above the scan.
    Unsupported,
}

impl From<PushdownClass> for TableProviderFilterPushDown {
    fn from(class: PushdownClass) -> TableProviderFilterPushDown {
        match class {
            PushdownClass::Exact => TableProviderFilterPushDown::Exact,
            PushdownClass::InexactCandidate => TableProviderFilterPushDown::Inexact,
            PushdownClass::Unsupported => TableProviderFilterPushDown::Unsupported,
        }
    }
}

/// One backend's relation binding: produce a `DataFusion` table provider
/// for a catalog relation at a bound snapshot. Returning `Ok(None)` means
/// the relation is empty for this backend/snapshot (still queryable).
pub trait RelationProviderFactory: Send + Sync {
    fn provider(
        &self,
        relation: &RelationDef,
        snapshot: &Snapshot,
    ) -> Result<Option<Arc<dyn TableProvider>>, QueryError>;
}

/// Does this expression involve virtual value work (a planted internal
/// function)? Such expressions never reach a provider.
fn involves_value_work(expr: &Expr) -> bool {
    use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
    let mut found = false;
    let _ = expr.apply(|node| {
        if let Expr::ScalarFunction(call) = node
            && call
                .func
                .name()
                .starts_with(crate::value::lowering::INTERNAL_FN_PREFIX)
        {
            found = true;
            return Ok(TreeNodeRecursion::Stop);
        }
        Ok(TreeNodeRecursion::Continue)
    });
    found
}

/// The trusted wrapper around a backend's table provider.
#[derive(Debug)]
pub struct TrustedRelation {
    schema: SchemaRef,
    inner: Option<Arc<dyn TableProvider>>,
}

impl TrustedRelation {
    /// Wrap `inner` (or an always-empty relation) with the catalog schema
    /// as the public truth.
    #[must_use]
    pub fn new(relation: &RelationDef, inner: Option<Arc<dyn TableProvider>>) -> TrustedRelation {
        TrustedRelation {
            schema: relation.schema(),
            inner,
        }
    }
}

#[async_trait]
impl TableProvider for TrustedRelation {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DfResult<Vec<TableProviderFilterPushDown>> {
        let Some(inner) = &self.inner else {
            return Ok(vec![
                TableProviderFilterPushDown::Unsupported;
                filters.len()
            ]);
        };
        // Value predicates are never a provider's business.
        let inner_answers = inner.supports_filters_pushdown(filters)?;
        Ok(filters
            .iter()
            .zip(inner_answers)
            .map(|(filter, answer)| {
                if involves_value_work(filter) {
                    TableProviderFilterPushDown::Unsupported
                } else {
                    answer
                }
            })
            .collect())
    }

    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        match &self.inner {
            Some(inner) => {
                // Defense in depth: even if DataFusion hands us a value
                // filter (it should not, per supports_filters_pushdown),
                // the provider never sees it.
                let resident: Vec<Expr> = filters
                    .iter()
                    .filter(|f| !involves_value_work(f))
                    .cloned()
                    .collect();
                inner.scan(state, projection, &resident, limit).await
            }
            None => {
                let projected = match projection {
                    Some(indices) => Arc::new(self.schema.project(indices)?),
                    None => self.schema.clone(),
                };
                Ok(Arc::new(datafusion::physical_plan::empty::EmptyExec::new(
                    projected,
                )))
            }
        }
    }
}
