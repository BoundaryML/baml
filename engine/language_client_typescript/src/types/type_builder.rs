// This file provides the native bindings between our Rust implementation and TypeScript
// We use NAPI-RS to expose Rust functionality to JavaScript/TypeScript
use baml_runtime::type_builder::{self, WithMeta};
use baml_types::BamlValue;
use napi_derive::napi;

// Create TypeScript-compatible wrappers for our Rust types
// These macros generate the necessary code for TypeScript interop
crate::lang_wrapper!(TypeBuilder, type_builder::TypeBuilder);

// Thread-safe wrapper for EnumBuilder with name tracking
// The sync_thread_safe attribute ensures safe concurrent access from TypeScript
crate::lang_wrapper!(EnumBuilder, type_builder::EnumBuilder, sync_thread_safe, name: String);

// Thread-safe wrapper for ClassBuilder with name tracking
// Enables safe TypeScript interop with class definitions
crate::lang_wrapper!(ClassBuilder, type_builder::ClassBuilder, sync_thread_safe, name: String);

// Thread-safe wrapper for EnumValueBuilder
// Ensures enum value definitions can be safely accessed across threads
crate::lang_wrapper!(
    EnumValueBuilder,
    type_builder::EnumValueBuilder,
    sync_thread_safe
);

// Thread-safe wrapper for ClassPropertyBuilder
// Enables concurrent access to class property definitions
crate::lang_wrapper!(
    ClassPropertyBuilder,
    type_builder::ClassPropertyBuilder,
    sync_thread_safe
);

// Thread-safe wrapper for FieldType
// Core type system representation with thread-safety guarantees
crate::lang_wrapper!(FieldType, baml_types::FieldType, sync_thread_safe);

// Implement Default for TypeBuilder to allow easy instantiation
// This enables idiomatic Rust usage while maintaining TypeScript compatibility
impl Default for TypeBuilder {
    fn default() -> Self {
        Self::new()
    }
}


// note: you may notice a rust-analyzer warning in vs code when working with this file.
// the warning "did not find struct napitypebuilder parsed before expand #[napi] for impl"
// is a known false positive that occurs due to how rust-analyzer processes macro state.
//
// don't worry - the code compiles and works correctly! the warning is yet to be addressed by napi maintainers.
//
// if you'd like to hide this warning in vs code, you can add this to your settings.json:
//   "rust-analyzer.diagnostics.disabled": ["macro-error"]
//
// ref:
// https://github.com/napi-rs/napi-rs/issues/1630




#[napi]
impl TypeBuilder {
    #[napi(constructor)]
    pub fn new() -> Self {
        type_builder::TypeBuilder::new().into()
    }

    #[napi]
    pub fn get_enum(&self, name: String) -> EnumBuilder {
        EnumBuilder {
            inner: self.inner.r#enum(&name),
            name,
        }
    }

    #[napi]
    pub fn get_class(&self, name: String) -> ClassBuilder {
        ClassBuilder {
            inner: self.inner.class(&name),
            name,
        }
    }

    #[napi]
    pub fn list(&self, inner: &FieldType) -> FieldType {
        inner.inner.lock().unwrap().clone().as_list().into()
    }

    #[napi]
    pub fn optional(&self, inner: &FieldType) -> FieldType {
        inner.inner.lock().unwrap().clone().as_optional().into()
    }

    #[napi]
    pub fn string(&self) -> FieldType {
        baml_types::FieldType::string().into()
    }

    #[napi]
    pub fn literal_string(&self, value: String) -> FieldType {
        baml_types::FieldType::literal_string(value).into()
    }

    #[napi]
    pub fn literal_int(&self, value: i64) -> FieldType {
        baml_types::FieldType::literal_int(value).into()
    }

    #[napi]
    pub fn literal_bool(&self, value: bool) -> FieldType {
        baml_types::FieldType::literal_bool(value).into()
    }

    #[napi]
    pub fn int(&self) -> FieldType {
        baml_types::FieldType::int().into()
    }

    #[napi]
    pub fn float(&self) -> FieldType {
        baml_types::FieldType::float().into()
    }

    #[napi]
    pub fn bool(&self) -> FieldType {
        baml_types::FieldType::bool().into()
    }

    #[napi]
    pub fn null(&self) -> FieldType {
        baml_types::FieldType::null().into()
    }

    #[napi]
    pub fn map(&self, key: &FieldType, value: &FieldType) -> FieldType {
        baml_types::FieldType::map(
            key.inner.lock().unwrap().clone(),
            value.inner.lock().unwrap().clone(),
        )
        .into()
    }

    #[napi]
    pub fn union(&self, types: Vec<&FieldType>) -> FieldType {
        baml_types::FieldType::union(
            types
                .iter()
                .map(|t| t.inner.lock().unwrap().clone())
                .collect(),
        )
        .into()
    }

    #[napi]
    pub fn to_string(&self) -> String {
        self.inner.to_string()
    }
}

#[napi]
impl FieldType {
    #[napi]
    pub fn list(&self) -> FieldType {
        self.inner.lock().unwrap().clone().as_list().into()
    }

    #[napi]
    pub fn optional(&self) -> FieldType {
        self.inner.lock().unwrap().clone().as_optional().into()
    }
}

#[napi]
impl EnumBuilder {
    #[napi]
    pub fn value(&self, name: String) -> EnumValueBuilder {
        self.inner.lock().unwrap().value(&name).into()
    }

    #[napi]
    pub fn alias(&self, alias: Option<&str>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| {
                BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }

    #[napi]
    pub fn field(&self) -> FieldType {
        baml_types::FieldType::r#enum(&self.name).into()
    }
}

#[napi]
impl EnumValueBuilder {
    #[napi]
    pub fn alias(&self, alias: Option<&str>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| {
                BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }

    #[napi]
    pub fn skip(&self, skip: Option<bool>) -> Self {
        self.inner
            .lock()
            .unwrap()
            .with_meta("skip", skip.map_or(BamlValue::Null, BamlValue::Bool));
        self.inner.clone().into()
    }

    #[napi]
    pub fn description(&self, description: Option<&str>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "description",
            description.map_or(baml_types::BamlValue::Null, |s| {
                BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }
}

#[napi]
impl ClassBuilder {
    #[napi]
    pub fn field(&self) -> FieldType {
        baml_types::FieldType::class(&self.name).into()
    }

    #[napi]
    pub fn property(&self, name: String) -> ClassPropertyBuilder {
        self.inner.lock().unwrap().property(&name).into()
    }
}

#[napi]
impl ClassPropertyBuilder {
    #[napi]
    pub fn set_type(&self, field_type: &FieldType) -> Self {
        self.inner
            .lock()
            .unwrap()
            .r#type(field_type.inner.lock().unwrap().clone());
        self.inner.clone().into()
    }

    #[napi]
    pub fn alias(&self, alias: Option<&str>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| {
                BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }

    #[napi]
    pub fn description(&self, description: Option<&str>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "description",
            description.map_or(baml_types::BamlValue::Null, |s| {
                BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }
}
