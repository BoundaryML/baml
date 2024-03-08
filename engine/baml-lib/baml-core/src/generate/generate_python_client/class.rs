use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContentPy as WithFileContent,
    generate_python_client::field_type::{
        can_be_null, to_internal_type, to_internal_type_constructor, to_type_check,
    },
    ir::{Class, Expression, FieldType, Walker},
};

use super::{
    python_language_features::{PythonFileCollector, PythonLanguageFeatures, ToPython},
    template::render_with_hbs,
};

impl WithFileContent<PythonLanguageFeatures> for Walker<'_, &Class> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "types".into()
    }

    fn write(&self, collector: &mut PythonFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);

        let static_fields_json = self
            .elem()
            .static_fields
            .iter()
            .map(|f| {
                json!({
                    "name": f.elem.name,
                    "type": f.elem.r#type.elem.to_py(),
                    "type_partial": f.elem.r#type.elem.to_py(),
                    "optional": match f.elem.r#type.elem {
                        FieldType::Optional(_) => true,
                        _ => false,
                    },
                    "can_be_null": can_be_null(&f.elem.r#type.elem),
                    "alias": f.attributes.get("alias").map(|s| s.to_py()),
                })
            })
            .collect::<Vec<_>>();

        let dynamic_fields_json = self.elem().dynamic_fields.iter().map(|f| {
            json!({
                "name": f.elem.name,
                "type": f.elem.r#type.elem.to_py(),
                "type_partial": f.elem.r#type.elem.to_py(),
                "optional": match f.elem.r#type.elem {
                    FieldType::Optional(_) => true,
                    _ => false,
                },
                "code": f.attributes.get("get/python").map_or("raise NotImplemented()".to_string(), |v| match v {
                    Expression::RawString(s) => s.clone(),
                    Expression::String(s) => s.clone(),
                    _ => "raise NotImplemented()".to_string(),
                }),
            })
        }).collect::<Vec<_>>();

        file.append(render_with_hbs(
            super::template::Template::Class,
            &json!({
                "name": self.elem().name,
                "name_partial": "Partial".to_string() + &self.elem().name,
                "fields": static_fields_json,
                "properties": dynamic_fields_json,
                "num_fields": self.elem().static_fields.len() + self.elem().dynamic_fields.len(),
            }),
        ));
        file.add_export(self.elem().name.clone());
        collector.finish_file();

        // let file = collector.start_file(self.file_dir(), self.file_name() + "_internal", false);
        // file.add_import("./types", self.elem().name.clone(), None, false);
        // file.add_export(format!("Internal{}", self.elem().name));
        // file.append(render_with_hbs(
        //     super::template::Template::ClassInternal,
        //     &json!({
        //         "name": self.elem().name,
        //         "fields": self.elem().static_fields.iter().map(|f| json!({
        //             "name": f.elem.name,
        //             "type": f.elem.r#type.elem.to_ts(),
        //             "internal_type": to_internal_type(&f.elem.r#type.elem),
        //             "constructor": to_internal_type_constructor(&format!("data.{}", f.elem.name), &f.elem.r#type.elem),
        //             "check": to_type_check(&format!("obj.{}", f.elem.name), &f.elem.r#type.elem),
        //         })).collect::<Vec<_>>(),
        //         "getters": self.elem().dynamic_fields.iter().map(|f| json!({
        //             "name": f.elem.name,
        //             "type": f.elem.r#type.elem.to_ts(),
        //             "internal_type": to_internal_type(&f.elem.r#type.elem),
        //             "body": f.attributes.get("get/typescript").map_or("throw NotImplemented()".to_string(), |v| match v {
        //                 Expression::RawString(s) => s.clone(),
        //                 Expression::String(s) => s.clone(),
        //                 _ => "throw NotImplemented()".to_string(),
        //             }),
        //         })).collect::<Vec<_>>(),
        //     }),
        // ));
        // collector.finish_file();
    }
}
