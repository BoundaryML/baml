use crate::ir::TypeAlias;

use super::Signature;

impl Signature for TypeAlias {
    fn type_name(&self) -> &'static str {
        "type_alias"
    }

    fn interface(&self) -> Option<String> {
        let mut content = self.elem.name.clone();
        content.push_str("--TYPE--");
        Some(content)
    }

    // Typeically don't have an impl
}
