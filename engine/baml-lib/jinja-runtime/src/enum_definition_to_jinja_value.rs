use baml_types::EvaluationContext;
use internal_baml_core::ir::repr::IntermediateRepr;

use crate::{
    baml_value_to_jinja_value::IntoMiniJinjaValue,
    types::{Enum, Name},
};

impl IntoMiniJinjaValue for Enum {
    fn to_minijinja_value(
        &self,
        _ir: &IntermediateRepr,
        _eval_ctx: &EvaluationContext<'_>,
    ) -> minijinja::Value {
        let enum_definition = MinijinjaTypeEnum {
            name: self.name.clone(),
            values: self
                .values
                .iter()
                .map(|v| MinijinjaTypeEnumValue { name: v.0.clone() })
                .collect(),
        };

        minijinja::Value::from_object(enum_definition)
    }
}

struct MinijinjaTypeEnum {
    name: Name,
    // name + possible alias
    values: Vec<MinijinjaTypeEnumValue>,
}

impl std::fmt::Display for MinijinjaTypeEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "<enum {}>", self.name.real_name())
    }
}

impl std::fmt::Debug for MinijinjaTypeEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl minijinja::value::Object for MinijinjaTypeEnum {
    fn kind(&self) -> minijinja::value::ObjectKind<'_> {
        minijinja::value::ObjectKind::Struct(self)
    }
}

impl minijinja::value::StructObject for MinijinjaTypeEnum {
    fn get_field(&self, name: &str) -> Option<minijinja::Value> {
        self.values
            .iter()
            .find(|v| v.name.real_name() == name)
            .map(|v| minijinja::Value::from_object(v.clone()))
    }
}

#[derive(Clone)]
struct MinijinjaTypeEnumValue {
    name: Name,
}

impl std::fmt::Display for MinijinjaTypeEnumValue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.name.rendered_name())
    }
}

impl minijinja::value::StructObject for MinijinjaTypeEnumValue {
    fn get_field(&self, name: &str) -> Option<minijinja::Value> {
        match name {
            "name" => Some(minijinja::Value::from(self.name.real_name().to_string())),
            "alias" => Some(minijinja::Value::from(
                self.name.rendered_name().to_string(),
            )),
            _ => None,
        }
    }
}

impl std::fmt::Debug for MinijinjaTypeEnumValue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl minijinja::value::Object for MinijinjaTypeEnumValue {
    fn kind(&self) -> minijinja::value::ObjectKind<'_> {
        minijinja::value::ObjectKind::Struct(self)
    }
}

impl PartialEq for MinijinjaTypeEnumValue {
    fn eq(&self, other: &Self) -> bool {
        println!(
            "eq: {} == {}",
            self.name.rendered_name(),
            other.name.rendered_name()
        );
        self.name.rendered_name() == other.name.rendered_name()
    }
}

impl PartialEq<String> for MinijinjaTypeEnumValue {
    fn eq(&self, other: &String) -> bool {
        println!("eq: {} == {}", self.name.rendered_name(), other);
        self.name.rendered_name() == *other
    }
}
