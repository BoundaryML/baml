use std::ops::Deref;

use baml_runtime::type_builder::{self, WithMeta};
use baml_types::{ir_type::UnionConstructor, BamlValue};
use pyo3::{
    prelude::PyAnyMethods,
    pymethods,
    types::{PyTuple, PyTupleMethods},
    Bound, PyResult,
};

use crate::errors::BamlError;

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
crate::lang_wrapper!(FieldType, baml_types::TypeIR, sync_thread_safe);

impl Default for TypeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[pymethods]
impl TypeBuilder {
    #[new]
    pub fn new() -> Self {
        type_builder::TypeBuilder::new().into()
    }

    pub fn reset(&self) {
        self.inner.reset();
    }

    /// provides a detailed string representation of the typebuilder for python users.
    ///
    /// this method exposes the rust-implemented string formatting to python, ensuring
    /// consistent and professional output across both languages. the representation
    /// includes a complete view of:
    ///
    /// * all defined classes with their properties
    /// * all defined enums with their values
    /// * metadata such as aliases and descriptions
    /// * type information for properties
    ///
    /// the output format is carefully structured for readability, making it quite easy :D
    /// to understand the complete type hierarchy at a glance.
    pub fn __str__(&self) -> String {
        self.inner.to_string()
    }

    pub fn r#enum(&self, name: &str) -> EnumBuilder {
        EnumBuilder {
            inner: self.inner.upsert_enum(name),
            name: name.to_string(),
        }
    }

    // Rename to "class_"
    #[pyo3(name = "class_")]
    pub fn class(&self, name: &str) -> ClassBuilder {
        ClassBuilder {
            inner: self.inner.upsert_class(name),
            name: name.to_string(),
        }
    }

    pub fn literal_string(&self, value: &str) -> FieldType {
        baml_types::TypeIR::literal_string(value.to_string()).into()
    }

    pub fn literal_int(&self, value: i64) -> FieldType {
        baml_types::TypeIR::literal_int(value).into()
    }

    pub fn literal_bool(&self, value: bool) -> FieldType {
        baml_types::TypeIR::literal_bool(value).into()
    }

    pub fn list(&self, inner: &FieldType) -> FieldType {
        inner.inner.lock().unwrap().clone().as_list().into()
    }

    pub fn optional(&self, inner: &FieldType) -> FieldType {
        inner.inner.lock().unwrap().clone().as_optional().into()
    }

    pub fn string(&self) -> FieldType {
        baml_types::TypeIR::string().into()
    }

    pub fn int(&self) -> FieldType {
        baml_types::TypeIR::int().into()
    }

    pub fn float(&self) -> FieldType {
        baml_types::TypeIR::float().into()
    }

    pub fn bool(&self) -> FieldType {
        baml_types::TypeIR::bool().into()
    }

    pub fn null(&self) -> FieldType {
        baml_types::TypeIR::null().into()
    }

    pub fn map(&self, key: &FieldType, value: &FieldType) -> FieldType {
        baml_types::TypeIR::map(
            key.inner.lock().unwrap().clone(),
            value.inner.lock().unwrap().clone(),
        )
        .into()
    }

    #[pyo3(signature = (*types))]
    pub fn union(&self, types: &Bound<'_, PyTuple>) -> PyResult<FieldType> {
        let mut rs_types = vec![];
        for idx in 0..types.len() {
            let item = types.get_item(idx)?;
            let item = item.downcast::<FieldType>()?;
            rs_types.push(item.borrow().inner.lock().unwrap().clone());
        }
        Ok(baml_types::TypeIR::union(rs_types).into())
    }

    pub fn add_baml(
        &self,
        baml: &str,
        rt: &crate::runtime::BamlRuntime,
    ) -> Result<(), pyo3::PyErr> {
        self.inner
            .add_baml(baml, rt.inner.internal())
            .map_err(BamlError::from_anyhow)
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

    pub fn __eq__(&self, other: &FieldType) -> bool {
        self.inner.lock().unwrap().deref() == other.inner.lock().unwrap().deref()
    }
}

#[pymethods]
impl EnumBuilder {
    pub fn value(&self, name: &str) -> EnumValueBuilder {
        self.inner.lock().unwrap().upsert_value(name).into()
    }

    #[pyo3(signature = (alias = None))]
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
        baml_types::TypeIR::r#enum(&self.name).into()
    }
}

#[pymethods]
impl EnumValueBuilder {
    #[pyo3(signature = (alias = None))]
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

    #[pyo3(signature = (description = None))]
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
        baml_types::TypeIR::class(&self.name).into()
    }

    pub fn list_properties(&self) -> Vec<(String, ClassPropertyBuilder)> {
        self.inner
            .lock()
            .unwrap()
            .list_properties_key_value()
            .into_iter()
            .map(|(name, prop_builder)| (name, prop_builder.into()))
            .collect()
    }

    pub fn remove_property(&self, name: &str) {
        self.inner.lock().unwrap().remove_property(name);
    }

    pub fn reset(&self) {
        self.inner.lock().unwrap().reset();
    }

    pub fn property(&self, name: &str) -> ClassPropertyBuilder {
        self.inner.lock().unwrap().upsert_property(name).into()
    }
}

#[pymethods]
impl ClassPropertyBuilder {
    pub fn r#type(&self, r#type: &FieldType) -> Self {
        self.inner
            .lock()
            .unwrap()
            .set_type(r#type.inner.lock().unwrap().clone());
        self.inner.clone().into()
    }

    pub fn get_type(&self) -> PyResult<FieldType> {
        self.inner
            .lock()
            .unwrap()
            .r#type()
            .map(FieldType::from)
            .ok_or_else(|| BamlError::from_anyhow(anyhow::anyhow!(
                "attempted to read a property that has no defined type, this is likely an internal bug"
            )))
    }

    #[pyo3(signature = (alias = None))]
    pub fn alias(&self, alias: Option<&str>) -> Self {
        self.inner.lock().unwrap().with_meta(
            "alias",
            alias.map_or(baml_types::BamlValue::Null, |s| {
                BamlValue::String(s.to_string())
            }),
        );
        self.inner.clone().into()
    }

    #[pyo3(signature = (description = None))]
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
