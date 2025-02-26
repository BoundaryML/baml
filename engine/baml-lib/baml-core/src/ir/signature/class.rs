use baml_types::{BamlMediaType, Constraint, ConstraintLevel, LiteralValue};

use crate::ir::{repr::{Class, Field, Node}, FieldType, TypeValue};

use super::Signature;

impl Signature for Node<Class> {
    fn type_name(&self) -> &'static str {
        "class"
    }

    fn interface(&self) -> Option<String> {
        let mut content = self.elem.name.clone();
        content.push_str("--STATIC_FIELDS--");
        for field in self.elem.static_fields.iter() {
            field.interface().map(|s| content.push_str(&s));
        }
        for constraint in self.attributes.constraints.iter() {
            constraint.interface().map(|s| content.push_str(&s));
        }
        match self.attributes.get("dynamic") {
            Some(dynamic) => {
                content.push_str("--DYNAMIC--");
                dynamic.impl_().map(|s| content.push_str(&s));
            },
            None => (),
        }
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


        content.push_str("--STATIC_FIELDS--");
        for field in self.elem.static_fields.iter() {
            field.impl_().map(|s| {
                content.push_str("--STATIC_FIELD--");
                content.push_str(&s);
            });
        }
        Some(content)
    }
}

impl Signature for Node<Field> {
    fn type_name(&self) -> &'static str {
        "field"
    }
    
    fn interface(&self) -> Option<String> {
        let mut content = self.elem.name.clone();
        content.push_str("--TYPE--");
        self.elem.r#type.interface().map(|s| content.push_str(&s));
        Some(content)
    }

    fn impl_(&self) -> Option<String> {
        let mut content = self.attributes.get("alias").and_then(|alias| alias.impl_()).unwrap_or(self.elem.name.clone());
        
        self.attributes.get("description").map(|description| {
            description.impl_().map(|s| {
                content.push_str("--DESCRIPTION--");
                content.push_str(&s);
            });
        });
        Some(content)
    }
}

impl Signature for Constraint {
    fn type_name(&self) -> &'static str {
        "constraint"
    }
    
    fn interface(&self) -> Option<String> {
        let mut content = self.label.as_ref().map_or("", |l| l.as_str()).to_string();
        content.push_str("--LEVEL--");
        match self.level {
            ConstraintLevel::Check => content.push_str("check"),
            ConstraintLevel::Assert => content.push_str("assert"),
        }
        content.push_str("--EXPRESSION--");
        self.expression.interface().map(|s| content.push_str(&s));
        Some(content)
    }
}

impl Signature for Node<FieldType> {
    fn type_name(&self) -> &'static str {
        "field_type"
    }
    
    fn interface(&self) -> Option<String> {
        self.elem.interface()
    }
}

impl Signature for FieldType {
    fn type_name(&self) -> &'static str {
        match self {
            baml_types::FieldType::Primitive(type_value) => type_value.type_name(),
            baml_types::FieldType::Enum(_) => "enum",
            baml_types::FieldType::Literal(literal_value) => literal_value.type_name(),
            baml_types::FieldType::Class(_) => "class",
            baml_types::FieldType::List(field_type) => field_type.type_name(),
            baml_types::FieldType::Map(field_type, field_type1) => "map",
            baml_types::FieldType::Union(field_types) => "union",
            baml_types::FieldType::Tuple(field_types) => "tuple",
            baml_types::FieldType::Optional(field_type) => "optional",
            baml_types::FieldType::RecursiveTypeAlias(_) => "recursive_type_alias",
            baml_types::FieldType::WithMetadata { base, constraints, streaming_behavior } => "with_metadata",
        }
    }
    
    fn interface(&self) -> Option<String> {
        match self {
            baml_types::FieldType::Primitive(type_value) => type_value.interface(),
            baml_types::FieldType::Enum(name) => Some(name.clone()),
            baml_types::FieldType::Literal(literal_value) => literal_value.interface(),
            baml_types::FieldType::Class(name) => Some(name.clone()),
            baml_types::FieldType::List(field_type) => (*field_type).interface(),
            baml_types::FieldType::Map(field_type, field_type1) => {
                let mut content = "--KEY--".to_string();
                field_type.interface().map(|s| content.push_str(&s));
                content.push_str("--VALUE--");
                field_type1.interface().map(|s| content.push_str(&s));
                Some(content)
            },
            baml_types::FieldType::Union(field_types) => {
                let mut content = "--TYPES--".to_string();
                for ft in field_types.iter() {
                    content.push_str("--TYPE--");
                    ft.interface().map(|s| content.push_str(&s));
                }
                Some(content)
            },
            baml_types::FieldType::Tuple(field_types) => {
                let mut content = "--TYPES--".to_string();
                for ft in field_types.iter() {
                    content.push_str("--TYPE--");
                    ft.interface().map(|s| content.push_str(&s));
                }
                Some(content)
            },
            baml_types::FieldType::Optional(field_type) => {
                let mut content = "--TYPE--".to_string();
                field_type.interface().map(|s| content.push_str(&s));
                Some(content)
            },
            baml_types::FieldType::RecursiveTypeAlias(name) => Some(name.clone()),
            baml_types::FieldType::WithMetadata { base, constraints, streaming_behavior } => {
                let mut content = "--BASE--".to_string();
                base.interface().map(|s| content.push_str(&s));
                content.push_str("--CONSTRAINTS--");
                for constraint in constraints.iter() {
                    constraint.interface().map(|s| content.push_str(&s));
                }
                // STREAMING BEHAVIOR deliberately not included
                Some(content)
            },
        }
    }
}

impl Signature for LiteralValue {
    fn type_name(&self) -> &'static str {
        match self {
            LiteralValue::String(_) => "literal_string",
            LiteralValue::Int(_) => "literal_int",
            LiteralValue::Bool(_) => "literal_bool",
        }
    }
    
    fn interface(&self) -> Option<String> {
        match self {
            LiteralValue::String(string) => Some(string.clone()),
            LiteralValue::Int(int) => Some(int.to_string()),
            LiteralValue::Bool(bool) => Some(bool.to_string()),
        }
    }    
}

impl Signature for TypeValue {
    fn type_name(&self) -> &'static str {
        "type_value"
    }
    
    fn interface(&self) -> Option<String> {
        match &self {
            baml_types::TypeValue::String => Some("string".to_string()),
            baml_types::TypeValue::Int => Some("int".to_string()),
            baml_types::TypeValue::Float => Some("float".to_string()),
            baml_types::TypeValue::Bool => Some("bool".to_string()),
            baml_types::TypeValue::Null => Some("null".to_string()),
            baml_types::TypeValue::Media(baml_media_type) => baml_media_type.interface(),
        }
    }
}

impl Signature for BamlMediaType {
    fn type_name(&self) -> &'static str {
        "baml_media_type"
    }
    
    fn interface(&self) -> Option<String> {
        match self {
            BamlMediaType::Image => Some("image".to_string()),
            BamlMediaType::Audio => Some("audio".to_string()),
        }
    }
}
