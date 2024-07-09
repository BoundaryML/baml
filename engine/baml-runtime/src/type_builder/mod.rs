use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use baml_types::{BamlValue, FieldType};
use indexmap::IndexMap;
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_jinja::types::RenderOptions;
use std::collections::HashMap;

use crate::{
    internal::prompt_renderer,
    runtime_context::{PropertyAttributes, RuntimeClassOverride, RuntimeEnumOverride},
    RuntimeContext,
};

pub mod json_schema_unstable;
type MetaData = Arc<Mutex<IndexMap<String, BamlValue>>>;

trait Meta {
    fn meta(&self) -> MetaData;
}

pub trait WithMeta {
    fn with_meta(&self, key: &str, value: BamlValue) -> &Self;
}

macro_rules! impl_meta {
    ($type:ty) => {
        impl Meta for $type {
            fn meta(&self) -> MetaData {
                self.meta.clone()
            }
        }
    };
}

impl<T> WithMeta for T
where
    T: Meta,
{
    fn with_meta(&self, key: &str, value: BamlValue) -> &T {
        let meta = self.meta();
        let mut meta = meta.lock().unwrap();
        meta.insert(key.to_string(), value);
        self
    }
}

impl<T: Meta> From<&Arc<Mutex<T>>> for PropertyAttributes {
    fn from(value: &Arc<Mutex<T>>) -> Self {
        let value = value.lock().unwrap();
        let meta = value.meta();
        let meta = meta.lock().unwrap();
        let properties = meta.clone();
        let alias = properties.get("alias").cloned();
        let skip = properties.get("skip").and_then(|v| v.as_bool());

        Self {
            alias,
            skip,
            meta: properties,
        }
    }
}

pub struct ClassBuilder {
    properties: Arc<Mutex<IndexMap<String, Arc<Mutex<ClassPropertyBuilder>>>>>,
    meta: MetaData,
}
impl_meta!(ClassBuilder);

pub struct ClassPropertyBuilder {
    r#type: Arc<Mutex<Option<FieldType>>>,
    meta: MetaData,
}
impl_meta!(ClassPropertyBuilder);

impl ClassPropertyBuilder {
    pub fn r#type(&self, r#type: FieldType) -> &Self {
        *self.r#type.lock().unwrap() = Some(r#type);
        self
    }
}

impl ClassBuilder {
    pub fn new() -> Self {
        Self {
            properties: Default::default(),
            meta: Arc::new(Mutex::new(Default::default())),
        }
    }

    pub fn property(&self, name: &str) -> Arc<Mutex<ClassPropertyBuilder>> {
        let mut properties = self.properties.lock().unwrap();
        Arc::clone(properties.entry(name.to_string()).or_insert_with(|| {
            Arc::new(Mutex::new(ClassPropertyBuilder {
                r#type: Default::default(),
                meta: Default::default(),
            }))
        }))
    }
}

pub struct EnumBuilder {
    values: Arc<Mutex<IndexMap<String, Arc<Mutex<EnumValueBuilder>>>>>,
    meta: MetaData,
}
impl_meta!(EnumBuilder);

pub struct EnumValueBuilder {
    meta: MetaData,
}
impl_meta!(EnumValueBuilder);

impl EnumBuilder {
    pub fn new() -> Self {
        Self {
            values: Default::default(),
            meta: Arc::new(Mutex::new(Default::default())),
        }
    }

    pub fn value(&self, name: &str) -> Arc<Mutex<EnumValueBuilder>> {
        let mut values = self.values.lock().unwrap();
        Arc::clone(values.entry(name.to_string()).or_insert_with(|| {
            Arc::new(Mutex::new(EnumValueBuilder {
                meta: Default::default(),
            }))
        }))
    }
}

impl std::fmt::Debug for TypeBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Start the debug printout with the struct name
        write!(f, "TypeBuilder {{\n")?;

        // Safely attempt to acquire the lock and print classes
        write!(f, "  classes: ")?;
        match self.classes.lock() {
            Ok(classes) => {
                // We iterate through the keys only to avoid deadlocks and because we might not be able to print the values
                // safely without deep control over locking mechanisms
                let keys: Vec<_> = classes.keys().collect();
                write!(f, "{:?},\n", keys)?
            }
            Err(_) => write!(f, "Cannot acquire lock,\n")?,
        }

        // Safely attempt to acquire the lock and print enums
        write!(f, "  enums: ")?;
        match self.enums.lock() {
            Ok(enums) => {
                // Similarly, print only the keys
                let keys: Vec<_> = enums.keys().collect();
                write!(f, "{:?}\n", keys)?
            }
            Err(_) => write!(f, "Cannot acquire lock,\n")?,
        }

        // Close the struct printout
        write!(f, "}}")
    }
}

#[derive(Clone)]
pub struct TypeBuilder {
    classes: Arc<Mutex<IndexMap<String, Arc<Mutex<ClassBuilder>>>>>,
    enums: Arc<Mutex<IndexMap<String, Arc<Mutex<EnumBuilder>>>>>,
}

impl TypeBuilder {
    pub fn new() -> Self {
        Self {
            classes: Default::default(),
            enums: Default::default(),
        }
    }

    // pub fn output_format(&self) -> Result<String> {
    //     use crate::runtime_interface::InternalRuntimeInterface;

    //     let (classes, enums) = self.to_overrides();

    //     let ctx = crate::RuntimeContext {
    //         env: HashMap::new(),
    //         tags: HashMap::new(),
    //         class_override: classes,
    //         enum_overrides: enums,
    //     };
    //     let rendered = crate::BamlRuntime::from_file_content(
    //         "<virtual>",
    //         &[(
    //             "<prompt>.baml",
    //             r##"
    //     class OutputFormat {
    //         @@dynamic
    //     }
    //     function OutputFormat_Fn () -> OutputFormat {
    //         client PlaceholderClient
    //         // TODO: remove prefix text and suffix text
    //         prompt #"{{ ctx.output_format(prefix=null)}}"#
    //     }
    //     client<llm> PlaceholderClient {
    //         provider ollama
    //     }
    //     "##,
    //         )]
    //         .into_iter()
    //         .collect(),
    //         HashMap::<String, String>::new(),
    //     )?
    //     .internal()
    //     .render_prompt("OutputFormat_Fn", &ctx, &IndexMap::new(), None);

    //     let Ok((rendered, _)) = rendered else {
    //         anyhow::bail!("TODO internal error message (1)");
    //     };
    //     let internal_baml_jinja::RenderedPrompt::Chat(rendered) = rendered else {
    //         anyhow::bail!("TODO internal error message (2)");
    //     };
    //     let rendered = rendered
    //         .into_iter()
    //         .flat_map(|p| p.parts)
    //         .filter_map(|p| match p {
    //             internal_baml_jinja::ChatMessagePart::Text(t) => Some(t),
    //             _ => None,
    //         })
    //         .collect::<Vec<String>>()
    //         .join("<internal-error-3>");
    //     Ok(rendered)
    // }

    pub fn class(&self, name: &str) -> Arc<Mutex<ClassBuilder>> {
        Arc::clone(
            self.classes
                .lock()
                .unwrap()
                .entry(name.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(ClassBuilder::new()))),
        )
    }

    pub fn r#enum(&self, name: &str) -> Arc<Mutex<EnumBuilder>> {
        Arc::clone(
            self.enums
                .lock()
                .unwrap()
                .entry(name.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(EnumBuilder::new()))),
        )
    }

    pub fn to_overrides(
        &self,
    ) -> (
        IndexMap<String, RuntimeClassOverride>,
        IndexMap<String, RuntimeEnumOverride>,
    ) {
        log::debug!("Converting types to overrides");
        let cls = self
            .classes
            .lock()
            .unwrap()
            .iter()
            .map(|(name, cls)| {
                log::debug!("Converting class: {}", name);
                let mut overrides = RuntimeClassOverride {
                    alias: None,
                    new_fields: Default::default(),
                    update_fields: Default::default(),
                };

                cls.lock()
                    .unwrap()
                    .properties
                    .lock()
                    .unwrap()
                    .iter()
                    .for_each(|(property_name, f)| {
                        let attrs = PropertyAttributes::from(f);
                        let t = {
                            let property = f.lock().unwrap();
                            let t = property.r#type.lock().unwrap();
                            t.clone()
                        };
                        match t.as_ref() {
                            Some(r#type) => {
                                overrides
                                    .new_fields
                                    .insert(property_name.to_string(), (r#type.clone(), attrs));
                            }
                            None => {
                                overrides
                                    .update_fields
                                    .insert(property_name.to_string(), attrs);
                            }
                        }
                    });
                (name.clone(), overrides)
            })
            .collect();

        let enm = self
            .enums
            .lock()
            .unwrap()
            .iter()
            .map(|(name, enm)| {
                let attributes = PropertyAttributes::from(enm);
                let values = enm
                    .lock()
                    .unwrap()
                    .values
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(value_name, value)| {
                        (value_name.clone(), PropertyAttributes::from(value))
                    })
                    .collect();
                (
                    name.clone(),
                    RuntimeEnumOverride {
                        values,
                        alias: attributes.alias,
                    },
                )
            })
            .collect();
        log::debug!(
            "Dynamic types: \n {:#?} \n Dynamic enums\n {:#?} enums",
            cls,
            enm
        );
        (cls, enm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_builder() {
        let builder = TypeBuilder::new();
        let cls = builder.class("Person");
        let property = cls.lock().unwrap().property("name");
        property.lock().unwrap().r#type(FieldType::string());
        cls.lock()
            .unwrap()
            .property("age")
            .lock()
            .unwrap()
            .r#type(FieldType::int())
            .with_meta("alias", BamlValue::String("years".to_string()));
    }
}
