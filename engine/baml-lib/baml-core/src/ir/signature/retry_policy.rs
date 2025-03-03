use crate::ir::RetryPolicy;

use super::Signature;

impl Signature for RetryPolicy {
    fn type_name(&self) -> &'static str {
        "retry_policy"
    }

    fn interface(&self) -> Option<String> {
        None
    }
}