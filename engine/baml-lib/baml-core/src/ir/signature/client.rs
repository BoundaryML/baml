use internal_llm_client::ClientSpec;

use crate::ir::Client;

use super::Signature;

impl Signature for ClientSpec {
    fn type_name(&self) -> &'static str {
        "client_spec"
    }

    fn interface(&self) -> Option<String> {
        None
    }
}


impl Signature for Client {
    fn type_name(&self) -> &'static str {
        "client"
    }

    fn interface(&self) -> Option<String> {
        None
    }
}