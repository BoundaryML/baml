use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContentTs,
    generate_ts_client::field_type::{
        to_internal_type, to_internal_type_constructor, to_type_check,
    },
    ir::{Class, Expression, Walker},
};

use super::{
    template::render_with_hbs,
    ts_language_features::{TSFileCollector, TSLanguageFeatures, ToTypeScript},
};

impl WithFileContentTs<TSLanguageFeatures> for Walker<'_, &Class> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "types".into()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);
        file.append(render_with_hbs(
            super::template::Template::Class,
            &json!({
                "name": self.elem().name,
                "fields": self.elem().static_fields.iter().map(|f| json!({
                    "name": f.elem.name,
                    "type": f.elem.r#type.elem.to_ts(),
                })).collect::<Vec<_>>(),
            }),
        ));
        file.add_export(self.elem().name.clone());
        collector.finish_file();

        let file = collector.start_file(self.file_dir(), self.file_name() + "_internal", false);
        file.add_import("./types", self.elem().name.clone(), None, false);
        file.add_export(format!("Internal{}", self.elem().name));
        file.append(render_with_hbs(
            super::template::Template::ClassInternal,
            &json!({
                "name": self.elem().name,
                "fields": self.elem().static_fields.iter().map(|f| json!({
                    "name": f.elem.name,
                    "type": f.elem.r#type.elem.to_ts(),
                    "internal_type": to_internal_type(&f.elem.r#type.elem),
                    "constructor": to_internal_type_constructor(&format!("data.{}", f.elem.name), &f.elem.r#type.elem),
                    "check": to_type_check(&format!("obj.{}", f.elem.name), &f.elem.r#type.elem),
                })).collect::<Vec<_>>(),
                "getters": self.elem().dynamic_fields.iter().map(|f| json!({
                    "name": f.elem.name,
                    "type": f.elem.r#type.elem.to_ts(),
                    "internal_type": to_internal_type(&f.elem.r#type.elem),
                    "body": f.attributes.get("get/typescript").map_or("throw NotImplemented()".to_string(), |v| match v {
                        Expression::RawString(s) => s.clone(),
                        Expression::String(s) => s.clone(),
                        _ => "throw NotImplemented()".to_string(),
                    }),
                })).collect::<Vec<_>>(),
            }),
        ));
        collector.finish_file();
    }
}
