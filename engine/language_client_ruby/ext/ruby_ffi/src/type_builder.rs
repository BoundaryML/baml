use baml_runtime::type_builder::{self, WithMeta};
use magnus::typed_data::Obj;
use std::sync::{Arc, Mutex};

#[magnus::wrap(class = "Baml::TypeBuilder", free_immediately, size)]
#[derive(derive_more::From)]
struct TypeBuilder {
    inner: type_builder::TypeBuilder,
}

#[magnus::wrap(class = "Baml::EnumBuilder", free_immediately, size)]
#[derive(derive_more::From)]
struct EnumBuilder {
    name: String,
    inner: Arc<Mutex<type_builder::EnumBuilder>>,
}

#[magnus::wrap(class = "Baml::EnumValueBuilder", free_immediately, size)]
#[derive(derive_more::From)]
struct EnumValueBuilder {
    inner: Arc<Mutex<type_builder::EnumValueBuilder>>,
}

#[magnus::wrap(class = "Baml::ClassBuilder", free_immediately, size)]
#[derive(derive_more::From)]
struct ClassBuilder {
    name: String,
    inner: Arc<Mutex<type_builder::ClassBuilder>>,
}

#[magnus::wrap(class = "Baml::ClassPropertyBuilder", free_immediately, size)]
#[derive(derive_more::From)]
struct ClassPropertyBuilder {
    inner: Arc<Mutex<type_builder::ClassPropertyBuilder>>,
}

#[magnus::wrap(class = "Baml::FieldType", free_immediately, size)]
#[derive(derive_more::From)]
struct FieldType {
    inner: Arc<Mutex<baml_types::FieldType>>,
}

impl TypeBuilder {
    pub fn new() -> Self {
        type_builder::TypeBuilder::default().into()
    }

    pub fn add_enum(&self, name: &str) -> EnumBuilder {
        EnumBuilder {
            inner: self.inner.r#enum(name).into(),
            name: name.to_string(),
        }
    }

    pub fn add_class(&self, name: &str) -> ClassBuilder {
        ClassBuilder {
            name: name.to_string(),
            inner: self.inner.class(name),
        }
    }

    pub fn list(&self, inner: &FieldType) -> FieldType {
        inner.inner.lock().unwrap().clone().as_list().into()
    }

    pub fn optional(&self, inner: &FieldType) -> FieldType {
        inner.inner.lock().unwrap().clone().as_optional().into()
    }

    pub fn string(&self) -> FieldType {
        baml_types::FieldType::string().into()
    }

    pub fn int(&self) -> FieldType {
        baml_types::FieldType::int().into()
    }

    pub fn float(&self) -> FieldType {
        baml_types::FieldType::float().into()
    }

    pub fn bool(&self) -> FieldType {
        baml_types::FieldType::bool().into()
    }

    pub fn null(&self) -> FieldType {
        baml_types::FieldType::null().into()
    }
}

impl From<baml_types::FieldType> for FieldType {
    fn from(inner: baml_types::FieldType) -> Self {
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }
}

impl FieldType {
    pub fn list(&self) -> FieldType {
        self.inner.lock().unwrap().clone().as_list().into()
    }

    pub fn optional(&self) -> FieldType {
        self.inner.lock().unwrap().clone().as_optional().into()
    }
}

impl EnumBuilder {
    pub fn value(&self, name: &str) -> EnumValueBuilder {
        self.inner.lock().unwrap().value(name).into()
    }

    pub fn alias(rb_self: Obj<Self>, alias: Option<&str>) -> Obj<Self> {
        use std::ops::Deref;
        Obj::deref(&rb_self).inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| {
                baml_types::BamlValue::String(s.to_string())
            }),
        );
        rb_self
    }

    pub fn field(&self) -> FieldType {
        baml_types::FieldType::r#enum(&self.name).into()
    }
}

impl EnumValueBuilder {
    pub fn alias(&self, alias: Option<&str>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| {
                baml_types::BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }

    pub fn skip(&self, skip: Option<bool>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "skip",
            skip.map_or(baml_types::BamlValue::Null, baml_types::BamlValue::Bool),
        );
        self.inner.clone().into()
    }

    pub fn description(&self, description: Option<&str>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "description",
            description.map_or(baml_types::BamlValue::Null, |s| {
                baml_types::BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }
}

impl ClassBuilder {
    pub fn field(&self) -> FieldType {
        baml_types::FieldType::class(&self.name).into()
    }

    pub fn property(&self, name: &str) -> ClassPropertyBuilder {
        self.inner.lock().unwrap().property(name).into()
    }
}

impl ClassPropertyBuilder {
    pub fn r#type(&self, r#type: &FieldType) -> Self {
        self.inner
            .lock()
            .unwrap()
            .r#type(r#type.inner.lock().unwrap().clone());
        self.inner.clone().into()
    }

    pub fn alias(&self, alias: Option<&str>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| {
                baml_types::BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }

    pub fn description(&self, description: Option<&str>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "description",
            description.map_or(baml_types::BamlValue::Null, |s| {
                baml_types::BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }
}
