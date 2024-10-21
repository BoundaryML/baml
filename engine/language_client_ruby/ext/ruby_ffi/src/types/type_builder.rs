use crate::Result;
use baml_runtime::type_builder::{self, WithMeta};
use baml_types::BamlValue;
use magnus::{
    class, function, method, scan_args::scan_args, try_convert::TryConvertOwned, Module, Object,
    RModule, Value,
};

#[magnus::wrap(class = "Baml::Ffi::TypeBuilder", free_immediately, size)]
pub(crate) struct TypeBuilder {
    pub(crate) inner: type_builder::TypeBuilder,
}

crate::lang_wrapper!(EnumBuilder, "Baml::Ffi::EnumBuilder", type_builder::EnumBuilder, sync_thread_safe, name: String);
crate::lang_wrapper!(ClassBuilder, "Baml::Ffi::ClassBuilder", type_builder::ClassBuilder, sync_thread_safe, name: String);
crate::lang_wrapper!(
    EnumValueBuilder,
    "Baml::Ffi::EnumValueBuilder",
    type_builder::EnumValueBuilder,
    sync_thread_safe
);
crate::lang_wrapper!(
    ClassPropertyBuilder,
    "Baml::Ffi::ClassPropertyBuilder",
    type_builder::ClassPropertyBuilder,
    sync_thread_safe
);
crate::lang_wrapper!(
    FieldType,
    "Baml::Ffi::FieldType",
    baml_types::FieldType,
    sync_thread_safe
);

impl TypeBuilder {
    pub fn new() -> Self {
        Self {
            inner: type_builder::TypeBuilder::new(),
        }
    }

    pub fn r#enum(&self, name: String) -> EnumBuilder {
        EnumBuilder {
            inner: self.inner.r#enum(name.as_str()).into(),
            name: name.to_string(),
        }
    }

    pub fn class(&self, name: String) -> ClassBuilder {
        ClassBuilder {
            inner: self.inner.class(name.as_str()).into(),
            name: name.to_string(),
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

    pub fn literal_string(&self, value: String) -> FieldType {
        baml_types::FieldType::literal_string(value).into()
    }

    pub fn literal_int(&self, value: i64) -> FieldType {
        baml_types::FieldType::literal_int(value).into()
    }

    pub fn literal_bool(&self, value: bool) -> FieldType {
        baml_types::FieldType::literal_bool(value).into()
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

    pub fn map(&self, key: &FieldType, value: &FieldType) -> FieldType {
        baml_types::FieldType::map(
            key.inner.lock().unwrap().clone(),
            value.inner.lock().unwrap().clone(),
        )
        .into()
    }

    pub fn union(&self, args: &[Value]) -> Result<FieldType> {
        let args = scan_args::<(), (), _, (), (), ()>(args)?;
        let types: Vec<&FieldType> = args.splat;
        Ok(baml_types::FieldType::union(
            types
                .into_iter()
                .map(|t| t.inner.lock().unwrap().clone())
                .collect(),
        )
        .into())
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("TypeBuilder", class::object())?;

        cls.define_singleton_method("new", function!(TypeBuilder::new, 0))?;
        cls.define_method("enum", method!(TypeBuilder::r#enum, 1))?;
        // "class" is used by Kernel: https://ruby-doc.org/core-3.0.2/Kernel.html#method-i-class
        cls.define_method("class_", method!(TypeBuilder::class, 1))?;
        cls.define_method("list", method!(TypeBuilder::list, 1))?;
        cls.define_method("optional", method!(TypeBuilder::optional, 1))?;
        cls.define_method("string", method!(TypeBuilder::string, 0))?;
        cls.define_method("int", method!(TypeBuilder::int, 0))?;
        cls.define_method("float", method!(TypeBuilder::float, 0))?;
        cls.define_method("bool", method!(TypeBuilder::bool, 0))?;
        cls.define_method("null", method!(TypeBuilder::null, 0))?;
        cls.define_method("map", method!(TypeBuilder::map, 2))?;
        cls.define_method("union", method!(TypeBuilder::union, -1))?;
        cls.define_method("literal_string", method!(TypeBuilder::literal_string, 1))?;
        cls.define_method("literal_int", method!(TypeBuilder::literal_int, 1))?;
        cls.define_method("literal_bool", method!(TypeBuilder::literal_bool, 1))?;

        Ok(())
    }
}

impl FieldType {
    pub fn list(&self) -> FieldType {
        self.inner.lock().unwrap().clone().as_list().into()
    }

    pub fn optional(&self) -> FieldType {
        self.inner.lock().unwrap().clone().as_optional().into()
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("FieldType", class::object())?;

        cls.define_method("list", method!(FieldType::list, 0))?;
        cls.define_method("optional", method!(FieldType::optional, 0))?;

        Ok(())
    }
}

// magnus makes it non-ergonomic to convert a ScanArgsSplat into a Vec<&T> because Vec puts
// stuff on the heap, and moving Ruby-owned objects to the heap is very unsafe. It does so
// by bounding ScanArgsSplat using TryConvertOwned, which is not implemented for &TypedData,
// so we have to implement it ourselves. This is perfectly safe to do because FieldType does
// not have any references to Ruby objects.
unsafe impl TryConvertOwned for &FieldType {}

impl EnumBuilder {
    pub fn value(&self, name: String) -> EnumValueBuilder {
        self.inner.lock().unwrap().value(name.as_str()).into()
    }

    pub fn alias(&self, alias: Option<String>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| BamlValue::String(s)),
        );
        self.inner.clone().into()
    }

    pub fn field(&self) -> FieldType {
        baml_types::FieldType::r#enum(&self.name).into()
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("EnumBuilder", class::object())?;

        cls.define_method("value", method!(EnumBuilder::value, 1))?;
        cls.define_method("alias", method!(EnumBuilder::alias, 1))?;
        cls.define_method("field", method!(EnumBuilder::field, 0))?;

        Ok(())
    }
}

impl EnumValueBuilder {
    pub fn alias(&self, alias: Option<String>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| BamlValue::String(s)),
        );
        self.inner.clone().into()
    }

    // #[pyo3(signature = (skip = true))]
    pub fn skip(&self, skip: Option<bool>) -> Self {
        self.inner
            .lock()
            .unwrap()
            .with_meta("skip", skip.map_or(BamlValue::Null, BamlValue::Bool));
        self.inner.clone().into()
    }

    pub fn description(&self, description: Option<String>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "description",
            description.map_or(baml_types::BamlValue::Null, |s| BamlValue::String(s)),
        );
        self.inner.clone().into()
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("EnumValueBuilder", class::object())?;

        cls.define_method("alias", method!(EnumValueBuilder::alias, 1))?;
        cls.define_method("skip", method!(EnumValueBuilder::skip, 1))?;
        cls.define_method("description", method!(EnumValueBuilder::description, 1))?;

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

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("ClassBuilder", class::object())?;

        cls.define_method("field", method!(ClassBuilder::field, 0))?;
        cls.define_method("property", method!(ClassBuilder::property, 1))?;

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
            alias.map_or(baml_types::BamlValue::Null, |s| BamlValue::String(s)),
        );
        self.inner.clone().into()
    }

    pub fn description(&self, description: Option<String>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "description",
            description.map_or(baml_types::BamlValue::Null, |s| BamlValue::String(s)),
        );
        self.inner.clone().into()
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("ClassPropertyBuilder", class::object())?;

        cls.define_method("type", method!(ClassPropertyBuilder::r#type, 1))?;
        cls.define_method("alias", method!(ClassPropertyBuilder::alias, 1))?;
        cls.define_method("description", method!(ClassPropertyBuilder::description, 1))?;

        Ok(())
    }
}
