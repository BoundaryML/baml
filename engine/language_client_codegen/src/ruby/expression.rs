use baml_types::{BamlMediaType, StringOr, TypeValue, UnresolvedValue};

use super::ruby_language_features::ToRuby;

// impl ToRuby for UnresolvedValue {
//     fn to_ruby(&self) -> String {
//         match self {
//             UnresolvedValue::Array(values) => {
//                 format!(
//                     "[{}]",
//                     values
//                         .iter()
//                         .map(|v| v.to_ruby())
//                         .collect::<Vec<_>>()
//                         .join(", ")
//                 )
//             }
//             UnresolvedValue::Map(values) => {
//                 format!(
//                     "{{ {} }}",
//                     values
//                         .iter()
//                         .map(|(k, v)| format!("{}: {}", k.to_ruby(), v.to_ruby()))
//                         .collect::<Vec<_>>()
//                         .join(", ")
//                 )
//             }
//             UnresolvedValue::Identifier(idn) => match idn {
//                 Identifier::ENV(idn) => format!("process.env.{}", idn),
//                 Identifier::Local(k) => format!("\"{}\"", k.replace('"', "\\\"")),
//                 Identifier::Ref(r) => format!("\"{}\"", r.join(".")),
//                 Identifier::Primitive(p) => p.to_ruby(),
//             },
//             UnresolvedValue::String(val) => match val {
//                 StringOr::EnvVar(s) => format!("process.env.{}", s),
//                 StringOr::Value(v) => format!("\"{}\"", v.replace('"', "\\\"")),
//                 StringOr::JinjaExpression(jinja_expression) => ,
//             },
//             UnresolvedValue::Numeric(val) => val.clone(),
//             UnresolvedValue::Bool(val) => val.to_string(),
//         }
//     }
// }

impl ToRuby for TypeValue {
    fn to_ruby(&self) -> String {
        match self {
            TypeValue::Bool => "boolean",
            TypeValue::Float => "number",
            TypeValue::Int => "number",
            TypeValue::String => "string",
            TypeValue::Null => "null",
            TypeValue::Media(BamlMediaType::Image) => "Image",
            TypeValue::Media(BamlMediaType::Audio) => "Audio",
            TypeValue::Media(BamlMediaType::Pdf) => "Pdf",
            TypeValue::Media(BamlMediaType::Video) => "Video",
        }
        .to_string()
    }
}
