use baml_runtime::type_builder::{self, WithMeta};
use magnus::{class, function, method, typed_data::Obj, Module, Object, RModule};
use std::sync::{Arc, Mutex};

#[magnus::wrap(class = "Baml::TypeBuilder", free_immediately, size)]
#[derive(derive_more::From)]
pub struct TypeBuilder {
    pub(crate) inner: type_builder::TypeBuilder,
}

#[magnus::wrap(class = "Baml::EnumBuilder", free_immediately, size)]
#[derive(derive_more::From)]
pub struct EnumBuilder {
    name: String,
    inner: Arc<Mutex<type_builder::EnumBuilder>>,
}

#[magnus::wrap(class = "Baml::EnumValueBuilder", free_immediately, size)]
#[derive(derive_more::From)]
pub struct EnumValueBuilder {
    inner: Arc<Mutex<type_builder::EnumValueBuilder>>,
}

#[magnus::wrap(class = "Baml::ClassBuilder", free_immediately, size)]
#[derive(derive_more::From)]
pub struct ClassBuilder {
    name: String,
    inner: Arc<Mutex<type_builder::ClassBuilder>>,
}

#[magnus::wrap(class = "Baml::ClassPropertyBuilder", free_immediately, size)]
#[derive(derive_more::From)]
pub struct ClassPropertyBuilder {
    inner: Arc<Mutex<type_builder::ClassPropertyBuilder>>,
}

#[magnus::wrap(class = "Baml::FieldType", free_immediately, size)]
#[derive(derive_more::From)]
pub struct FieldType {
    inner: Arc<Mutex<baml_types::FieldType>>,
}

impl TypeBuilder {
    pub fn new() -> Self {
        type_builder::TypeBuilder::default().into()
    }

    // this is an upsert
    pub fn r#enum(&self, name: String) -> EnumBuilder {
        EnumBuilder {
            inner: self.inner.r#enum(name.as_str()).into(),
            name: name,
        }
    }

    // this is an upsert
    pub fn cls(&self, name: String) -> ClassBuilder {
        ClassBuilder {
            inner: self.inner.class(name.as_str()).into(),
            name: name,
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

impl crate::DefineInRuby for TypeBuilder {
    fn define_in_ruby(module: &RModule) -> crate::Result<()> {
        let class = module.define_class("TypeBuilder", class::object())?;

        class.define_singleton_method("new", function!(TypeBuilder::new, 0))?;

        class.define_method("enum", method!(TypeBuilder::r#enum, 1))?;
        class.define_method("cls", method!(TypeBuilder::cls, 1))?;
        class.define_method("list", method!(TypeBuilder::list, 1))?;
        class.define_method("optional", method!(TypeBuilder::optional, 1))?;

        class.define_method("string", method!(TypeBuilder::string, 0))?;
        class.define_method("int", method!(TypeBuilder::int, 0))?;
        class.define_method("float", method!(TypeBuilder::float, 0))?;
        class.define_method("bool", method!(TypeBuilder::bool, 0))?;
        class.define_method("null", method!(TypeBuilder::null, 0))?;

        Ok(())
    }
}

impl EnumBuilder {
    pub fn value(&self, name: String) -> EnumValueBuilder {
        self.inner.lock().unwrap().value(name.as_str()).into()
    }

    pub fn alias(rb_self: Obj<Self>, alias: Option<String>) -> Obj<Self> {
        use std::ops::Deref;
        Obj::deref(&rb_self).inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| {
                baml_types::BamlValue::String(s)
            }),
        );
        rb_self
    }

    pub fn field(&self) -> FieldType {
        baml_types::FieldType::r#enum(&self.name).into()
    }
}

impl crate::DefineInRuby for EnumBuilder {
    fn define_in_ruby(module: &RModule) -> crate::Result<()> {
        let class = module.define_class("EnumBuilder", class::object())?;
        class.define_method("value", method!(EnumBuilder::value, 1))?;
        class.define_method("alias", method!(EnumBuilder::alias, 1))?;
        class.define_method("field", method!(EnumBuilder::field, 0))?;

        Ok(())
    }
}

impl EnumValueBuilder {
    pub fn alias(&self, alias: Option<String>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| {
                baml_types::BamlValue::String(s)
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

    pub fn description(&self, description: Option<String>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "description",
            description.map_or(baml_types::BamlValue::Null, |s| {
                baml_types::BamlValue::String(s)
            }),
        );
        self.inner.clone().into()
    }
}

impl crate::DefineInRuby for EnumValueBuilder {
    fn define_in_ruby(module: &RModule) -> crate::Result<()> {
        let class = module.define_class("EnumBuilder", class::object())?;
        class.define_method("alias", method!(EnumValueBuilder::alias, 1))?;
        class.define_method("skip", method!(EnumValueBuilder::skip, 1))?;
        class.define_method("description", method!(EnumValueBuilder::description, 1))?;

        Ok(())
    }
}

impl ClassBuilder {
    pub fn field(&self) -> FieldType {
        baml_types::FieldType::class(&self.name).into()
    }

    pub fn property(&self, name: String) -> ClassPropertyBuilder {
        self.inner.lock().unwrap().property(name.as_str()).into()
    }
}

impl crate::DefineInRuby for ClassBuilder {
    fn define_in_ruby(module: &RModule) -> crate::Result<()> {
        let class = module.define_class("ClassBuilder", class::object())?;
        class.define_method("field", method!(ClassBuilder::field, 0))?;
        class.define_method("property", method!(ClassBuilder::property, 1))?;

        Ok(())
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

    pub fn alias(&self, alias: Option<String>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| {
                baml_types::BamlValue::String(s)
            }),
        );
        self.inner.clone().into()
    }

    pub fn description(&self, description: Option<String>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "description",
            description.map_or(baml_types::BamlValue::Null, |s| {
                baml_types::BamlValue::String(s)
            }),
        );
        self.inner.clone().into()
    }
}

impl crate::DefineInRuby for ClassPropertyBuilder {
    fn define_in_ruby(module: &RModule) -> crate::Result<()> {
        let class = module.define_class("ClassPropertyBuilder", class::object())?;
        class.define_method("type", method!(ClassPropertyBuilder::r#type, 1))?;
        class.define_method("alias", method!(ClassPropertyBuilder::alias, 1))?;
        class.define_method("description", method!(ClassPropertyBuilder::description, 1))?;

        Ok(())
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

impl crate::DefineInRuby for FieldType {
    fn define_in_ruby(module: &RModule) -> crate::Result<()> {
        let class = module.define_class("FieldType", class::object())?;
        class.define_method("list", method!(FieldType::list, 0))?;
        class.define_method("optional", method!(FieldType::optional, 0))?;

        Ok(())
    }
}
