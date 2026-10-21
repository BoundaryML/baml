//! Backend capability metadata (D4/D5).
//!
//! Allowlisted backend-specific functions register here with the backend
//! they require. Planning against a session bound to a different backend
//! fails with **`E_BACKEND_CAPABILITY`** before any data read — never a
//! silent local-to-hosted routing.

use std::{collections::HashMap, sync::Arc};

use datafusion::{
    arrow::datatypes::DataType,
    common::Result as DfResult,
    logical_expr::{
        ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
    },
};

use crate::{error::QueryError, scope::Backend};

/// The registry of backend-gated functions.
#[derive(Debug, Default, Clone)]
pub struct CapabilityRegistry {
    required: HashMap<String, Backend>,
}

impl CapabilityRegistry {
    #[must_use]
    pub fn new() -> CapabilityRegistry {
        CapabilityRegistry::default()
    }

    /// Register one allowlisted function as requiring `backend`.
    pub fn require(&mut self, function: impl Into<String>, backend: Backend) {
        self.required.insert(function.into(), backend);
    }

    /// Check one function reference against the session backend.
    pub fn check(&self, function: &str, current: Backend) -> Result<(), QueryError> {
        match self.required.get(function) {
            Some(required) if *required != current => Err(QueryError::backend_capability(
                function,
                required.as_str(),
                current.as_str(),
            )),
            _ => Ok(()),
        }
    }

    /// Planning stubs for every registered function: they give the parser
    /// a name/signature so the reference resolves, and the capability walk
    /// rejects the plan before execution could ever reach them.
    #[must_use]
    pub fn planning_stubs(&self) -> Vec<Arc<ScalarUDF>> {
        self.required
            .keys()
            .map(|name| Arc::new(ScalarUDF::from(CapabilityStub { name: name.clone() })))
            .collect()
    }

    #[must_use]
    pub fn is_registered(&self, function: &str) -> bool {
        self.required.contains_key(function)
    }
}

/// Name-only stub so backend-gated functions parse; execution is
/// unreachable (the capability walk rejects first).
#[derive(Debug, PartialEq, Eq, Hash)]
struct CapabilityStub {
    name: String,
}

impl ScalarUDFImpl for CapabilityStub {
    fn name(&self) -> &str {
        &self.name
    }
    fn signature(&self) -> &Signature {
        static SIG: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
        SIG.get_or_init(|| Signature::variadic_any(Volatility::Volatile))
    }
    fn return_type(&self, _args: &[DataType]) -> DfResult<DataType> {
        // A neutral type; the plan never executes.
        Ok(DataType::Float64)
    }
    fn invoke_with_args(&self, _args: ScalarFunctionArgs) -> DfResult<ColumnarValue> {
        datafusion::common::exec_err!(
            "{} requires a backend this session does not have; planning should \
             have rejected it (capability walk bug)",
            self.name
        )
    }
}
