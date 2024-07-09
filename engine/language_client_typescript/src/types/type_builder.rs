use baml_runtime::type_builder::{self, WithMeta};
use baml_types::BamlValue;
use napi::{bindgen_prelude::Reference, Env, JsObject, JsString, JsUnknown, ValueType};
use napi_derive::napi;

crate::lang_wrapper!(TypeBuilder, type_builder::TypeBuilder);
crate::lang_wrapper!(EnumBuilder, type_builder::EnumBuilder, sync_thread_safe, name: String);
crate::lang_wrapper!(ClassBuilder, type_builder::ClassBuilder, sync_thread_safe, name: String);
crate::lang_wrapper!(
    EnumValueBuilder,
    type_builder::EnumValueBuilder,
    sync_thread_safe
);
crate::lang_wrapper!(
    ClassPropertyBuilder,
    type_builder::ClassPropertyBuilder,
    sync_thread_safe
);
crate::lang_wrapper!(FieldType, baml_types::FieldType, sync_thread_safe);

#[napi_derive::napi]
struct TypeBuilderUnstableFeatures {
    inner: Reference<TypeBuilder>,
}

#[napi]
impl TypeBuilderUnstableFeatures {
    #[napi]
    pub fn add_json_schema(&self, env: Env, schema: JsUnknown) -> napi::Result<()> {
        use baml_runtime::type_builder::json_schema_unstable::AddJsonSchema;

        let value_type = schema.get_type()?;

        if value_type == ValueType::String {
            println!("adding json schema from string");
            let schema = env.from_js_value(schema)?;
            println!("adding json schema from string2");
            self.inner
                .inner
                .add_json_schema_from_str(schema)
                .map(|_| ())
                .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))
        } else {
            println!("adding json schema from value");
            let schema = env.from_js_value(schema)?;
            println!("adding json schema from value2");
            self.inner
                .inner
                .add_json_schema_from_value(schema)
                .map(|_| ())
                .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))
        }
    }
}

#[napi]
impl TypeBuilder {
    #[napi(constructor)]
    pub fn new() -> Self {
        type_builder::TypeBuilder::new().into()
    }

    #[napi(getter)]
    pub fn unstable_features(
        &self,
        nref: Reference<TypeBuilder>,
        env: Env,
    ) -> napi::Result<TypeBuilderUnstableFeatures> {
        Ok(TypeBuilderUnstableFeatures {
            inner: nref.clone(env)?,
        })
    }

    #[napi]
    pub fn get_enum(&self, name: String) -> EnumBuilder {
        EnumBuilder {
            inner: self.inner.r#enum(&name).into(),
            name,
        }
    }

    #[napi]
    pub fn get_class(&self, name: String) -> ClassBuilder {
        ClassBuilder {
            inner: self.inner.class(&name).into(),
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
