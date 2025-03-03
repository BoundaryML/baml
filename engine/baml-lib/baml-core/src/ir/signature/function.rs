use crate::ir::repr::{Function, Node};

use super::Signature;

impl Signature for Node<Function> {
    fn type_name(&self) -> &'static str {
        "function"
    }

    fn interface(&self) -> Option<String> {
        let mut content = self.elem.name.clone();
        content.push_str("--PARAMS--");
        for (name, field_type) in self.elem.inputs.iter() {
            content.push_str("--PARAM--");
            content.push_str(&name);
            content.push_str("--TYPE--");
            field_type.interface().map(|s| content.push_str(&s));
        }
        content.push_str("--RETURN--");
        self.elem.output.interface().map(|s| content.push_str(&s));
        Some(content)
    }


    fn impl_(&self) -> Option<String> {
        let Some(config) = self.elem.configs.iter().find(|config| config.name == self.elem.default_config) else {
            return None;
        };
        let mut content = "--CLIENT--".to_string();
        config.client.impl_().map(|s| content.push_str(&s));
        content.push_str("--PROMPT--");
        content.push_str(&config.prompt_template);
        Some(content)
    }
}
