use baml_runtime::type_builder::{self, WithMeta};
use baml_types::BamlValue;
use pyo3::{
    prelude::PyAnyMethods,
    pymethods,
    types::{PyTuple, PyTupleMethods},
    Bound, PyResult,
};
use pyo3::{PyObject, Python, ToPyObject};

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

#[pymethods]
impl TypeBuilder {
    #[new]
    pub fn new() -> Self {
        type_builder::TypeBuilder::new().into()
    }

    pub fn r#enum(&self, name: &str) -> EnumBuilder {
        EnumBuilder {
            inner: self.inner.r#enum(name).into(),
            name: name.to_string(),
        }
    }

    // Rename to "class_"
    #[pyo3(name = "class_")]
    pub fn class(&self, name: &str) -> ClassBuilder {
        ClassBuilder {
            inner: self.inner.class(name).into(),
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

    #[pyo3(signature = (*types))]
    pub fn union<'py>(&self, types: &Bound<'_, PyTuple>) -> PyResult<FieldType> {
        let mut rs_types = vec![];
        for idx in 0..types.len() {
            let item = types.get_item(idx)?;
            let item = item.downcast::<FieldType>()?;
            rs_types.push(item.borrow().inner.lock().unwrap().clone());
        }
        Ok(baml_types::FieldType::union(rs_types).into())
    }
}

#[pymethods]
impl FieldType {
    pub fn list(&self) -> FieldType {
        self.inner.lock().unwrap().clone().as_list().into()
    }

    pub fn optional(&self) -> FieldType {
        self.inner.lock().unwrap().clone().as_optional().into()
    }
}

#[pymethods]
impl EnumBuilder {
    pub fn value(&self, name: &str) -> EnumValueBuilder {
        self.inner.lock().unwrap().value(name).into()
    }

    pub fn alias(&self, alias: Option<&str>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| {
                BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }

    pub fn field(&self) -> FieldType {
        baml_types::FieldType::r#enum(&self.name).into()
    }
}

#[pymethods]
impl EnumValueBuilder {
    pub fn alias(&self, alias: Option<&str>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| {
                BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }

    #[pyo3(signature = (skip = true))]
    pub fn skip(&self, skip: Option<bool>) -> Self {
        self.inner
            .lock()
            .unwrap()
            .with_meta("skip", skip.map_or(BamlValue::Null, BamlValue::Bool));
        self.inner.clone().into()
    }

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

#[pymethods]
impl ClassBuilder {
    pub fn field(&self) -> FieldType {
        baml_types::FieldType::class(&self.name).into()
    }

    pub fn property(&self, name: &str) -> ClassPropertyBuilder {
        self.inner.lock().unwrap().property(name).into()
    }
}

#[pymethods]
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
                BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }

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
