use crate::ir::repr::{Enum, EnumValue, Node};

use super::Signature;

impl Signature for Node<Enum> {
    fn type_name(&self) -> &'static str {
        "enum"
    }

    fn interface(&self) -> Option<String> {
        let mut content = self.elem.name.clone();
        content.push_str("--VALUES--");
        for (value, _doc_string) in self.elem.values.iter() {
            value.interface().map(|s| content.push_str(&s));
        }
        self.attributes.get("dynamic").map(|dynamic| {
            dynamic.impl_().map(|s| {
                content.push_str("--DYNAMIC--");
                content.push_str(&s);
            });
        });
        Some(content)
    }

    fn impl_(&self) -> Option<String> {
        // get the alias
        let mut content = self.attributes.get("alias").and_then(|alias| alias.impl_()).unwrap_or(self.elem.name.clone());

        self.attributes.get("description").map(|description| {
            description.impl_().map(|s| {
                content.push_str("--DESCRIPTION--");
                content.push_str(&s);
            });
        });
        
        content.push_str("--VALUES--");
        for (value, _doc_string) in self.elem.values.iter() {
            value.impl_().map(|s| content.push_str(&s));
        }
        Some(content)
    }
}

impl Signature for Node<EnumValue> {
    fn type_name(&self) -> &'static str {
        "enum_value"
    }
    
    fn interface(&self) -> Option<String> {
        Some(self.elem.0.clone())
    }

    fn impl_(&self) -> Option<String> {
        let mut content = self.attributes.get("alias").and_then(|alias| alias.impl_()).unwrap_or(self.elem.0.clone());
        self.attributes.get("description").map(|description| {
            description.impl_().map(|s| {
                content.push_str("--DESCRIPTION--");
                content.push_str(&s);
            });
        });
        Some(content)
    }
}
