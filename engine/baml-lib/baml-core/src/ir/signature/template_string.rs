use crate::ir::TemplateString;

use super::Signature;

impl Signature for TemplateString {
    fn type_name(&self) -> &'static str {
        "template_string"
    }
    
    fn interface(&self) -> Option<String> {
        let mut content = self.elem.name.clone();
        content.push_str("--PARAMS--");
        for param in self.elem.params.iter() {
            content.push_str("--PARAM--");
            content.push_str(&param.name);
            content.push_str("--TYPE--");
            param.r#type.interface().map(|s| content.push_str(&s));
        }
        Some(content)
    }

    fn impl_(&self) -> Option<String> {
        let mut content = self.elem.name.clone();
        content.push_str("--TEMPLATE--");
        content.push_str(&self.elem.content);
        Some(content)
    }
}
