pub mod objects;

use baml_cffi_macros::export_baml_fn;
use baml_types::{ir_type::UnionConstructor, BamlValue, TypeIR};

use super::{BamlObjectResponse, BamlObjectResponseSuccess, CallMethod};
use crate::raw_ptr_wrapper::{
    ClassBuilderWrapper, ClassPropertyBuilderWrapper, EnumBuilderWrapper, EnumValueBuilderWrapper,
    TypeBuilderWrapper, TypeWrapper,
};

#[export_baml_fn]
impl TypeBuilderWrapper {
    #[export_baml_fn]
    fn string(&self) -> TypeIR {
        TypeIR::string()
    }

    #[export_baml_fn]
    fn int(&self) -> TypeIR {
        TypeIR::int()
    }

    #[export_baml_fn]
    fn float(&self) -> TypeIR {
        TypeIR::float()
    }

    #[export_baml_fn]
    fn bool(&self) -> TypeIR {
        TypeIR::bool()
    }

    #[export_baml_fn]
    fn literal_string(&self, value: &str) -> TypeIR {
        TypeIR::literal_string(value.to_string())
    }

    #[export_baml_fn]
    fn literal_int(&self, value: i64) -> TypeIR {
        TypeIR::literal_int(value)
    }

    #[export_baml_fn]
    fn literal_bool(&self, value: bool) -> TypeIR {
        TypeIR::literal_bool(value)
    }

    #[export_baml_fn]
    fn null(&self) -> TypeIR {
        TypeIR::null()
    }

    #[export_baml_fn]
    fn map(&self, key: &TypeWrapper, value: &TypeWrapper) -> TypeIR {
        TypeIR::map(key.as_ref().clone(), value.as_ref().clone())
    }

    #[export_baml_fn]
    fn list(&self, inner: &TypeWrapper) -> TypeIR {
        TypeIR::list(inner.as_ref().clone())
    }

    #[export_baml_fn]
    fn optional(&self, inner: &TypeWrapper) -> TypeIR {
        TypeIR::optional(inner.as_ref().clone())
    }

    #[export_baml_fn]
    #[allow(clippy::ptr_arg)]
    fn union(&self, types: &Vec<TypeWrapper>) -> TypeIR {
        TypeIR::union(types.iter().map(|t| t.as_ref().clone()).collect())
    }

    #[export_baml_fn]
    fn add_baml(&self, runtime: &baml_runtime::BamlRuntime, baml: &str) -> Result<(), String> {
        self.inner
            .add_baml(baml, runtime)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn add_enum(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        name: &str,
    ) -> Result<objects::EnumBuilder, String> {
        self.inner
            .add_enum(runtime, name)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn add_class(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        name: &str,
    ) -> Result<ClassBuilderWrapper, String> {
        self.inner
            .add_class(runtime, name)
            .map(ClassBuilderWrapper::from_object)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn class(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        name: &str,
    ) -> Result<ClassBuilderWrapper, String> {
        self.inner
            .class(runtime, name)
            .map(ClassBuilderWrapper::from_object)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn enum_(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        name: &str,
    ) -> Result<objects::EnumBuilder, String> {
        self.inner.r#enum(runtime, name).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn list_enums(&self, runtime: &baml_runtime::BamlRuntime) -> Vec<objects::EnumBuilder> {
        self.inner.list_enums(runtime)
    }

    #[export_baml_fn]
    fn list_classes(&self, runtime: &baml_runtime::BamlRuntime) -> Vec<objects::ClassBuilder> {
        self.inner.list_classes(runtime)
    }

    #[export_baml_fn]
    fn __display__(&self) -> String {
        self.type_builder.to_string()
    }
}

#[export_baml_fn]
impl TypeWrapper {
    #[export_baml_fn]
    fn list(&self) -> TypeIR {
        self.as_ref().clone().as_list()
    }

    #[export_baml_fn]
    fn optional(&self) -> TypeIR {
        self.as_ref().clone().as_optional()
    }

    #[export_baml_fn]
    fn __display__(&self) -> String {
        self.as_ref().to_string()
    }
}

#[export_baml_fn]
impl EnumBuilderWrapper {
    #[export_baml_fn]
    fn name(&self) -> String {
        self.inner.enum_name.to_string()
    }

    #[export_baml_fn]
    fn add_value(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        value: &str,
    ) -> Result<objects::EnumValueBuilder, String> {
        self.inner
            .add_value(runtime, value)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn set_description(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        description: &str,
    ) -> Result<(), String> {
        self.inner
            .set_description(runtime, description)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn set_alias(&self, runtime: &baml_runtime::BamlRuntime, alias: &str) -> Result<(), String> {
        self.inner
            .set_alias(runtime, alias)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn description(&self, runtime: &baml_runtime::BamlRuntime) -> Result<Option<String>, String> {
        self.inner.description(runtime).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn alias(&self, runtime: &baml_runtime::BamlRuntime) -> Result<Option<String>, String> {
        self.inner.alias(runtime).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn type_(&self, runtime: &baml_runtime::BamlRuntime) -> Result<TypeIR, String> {
        self.inner.r#type(runtime).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn list_values(
        &self,
        runtime: &baml_runtime::BamlRuntime,
    ) -> Result<Vec<objects::EnumValueBuilder>, String> {
        self.inner
            .list_values(runtime)
            .map(|builders| builders.into_iter().collect())
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn value(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        name: &str,
    ) -> Result<EnumValueBuilderWrapper, String> {
        self.inner
            .value(runtime, name)
            .map(EnumValueBuilderWrapper::from_object)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn is_from_ast(&self, runtime: &baml_runtime::BamlRuntime) -> Result<bool, String> {
        self.inner.is_from_ast(runtime).map_err(|e| e.to_string())
    }
}

#[export_baml_fn]
impl EnumValueBuilderWrapper {
    #[export_baml_fn]
    fn name(&self) -> String {
        self.inner.value_name.to_string()
    }

    #[export_baml_fn]
    fn set_description(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        description: &str,
    ) -> Result<(), String> {
        self.inner
            .set_description(runtime, description)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn set_alias(&self, runtime: &baml_runtime::BamlRuntime, alias: &str) -> Result<(), String> {
        self.inner
            .set_alias(runtime, alias)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn description(&self, runtime: &baml_runtime::BamlRuntime) -> Result<Option<String>, String> {
        self.inner.description(runtime).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn alias(&self, runtime: &baml_runtime::BamlRuntime) -> Result<Option<String>, String> {
        self.inner.alias(runtime).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn set_skip(&self, runtime: &baml_runtime::BamlRuntime, skip: bool) -> Result<(), String> {
        self.inner
            .set_skip(runtime, skip)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn skip(&self, runtime: &baml_runtime::BamlRuntime) -> Result<bool, String> {
        self.inner.skip(runtime).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn is_from_ast(&self, runtime: &baml_runtime::BamlRuntime) -> Result<bool, String> {
        self.inner.is_from_ast(runtime).map_err(|e| e.to_string())
    }
}

#[export_baml_fn]
impl ClassBuilderWrapper {
    #[export_baml_fn]
    fn name(&self) -> String {
        self.inner.class_name.to_string()
    }

    #[export_baml_fn]
    fn type_(&self, runtime: &baml_runtime::BamlRuntime) -> Result<TypeIR, String> {
        self.inner.r#type(runtime).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn list_properties(
        &self,
        runtime: &baml_runtime::BamlRuntime,
    ) -> Result<Vec<objects::ClassPropertyBuilder>, String> {
        self.inner
            .list_properties(runtime)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn set_alias(&self, runtime: &baml_runtime::BamlRuntime, alias: &str) -> Result<(), String> {
        self.inner
            .set_alias(runtime, alias)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn set_description(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        description: &str,
    ) -> Result<(), String> {
        self.inner
            .set_description(runtime, description)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn alias(&self, runtime: &baml_runtime::BamlRuntime) -> Result<Option<String>, String> {
        self.inner.alias(runtime).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn description(&self, runtime: &baml_runtime::BamlRuntime) -> Result<Option<String>, String> {
        self.inner.description(runtime).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn add_property(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        name: &str,
        field_type: &TypeWrapper,
    ) -> Result<objects::ClassPropertyBuilder, String> {
        self.inner
            .add_property(runtime, name, field_type.as_ref().clone())
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn property(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        name: &str,
    ) -> Result<objects::ClassPropertyBuilder, String> {
        self.inner
            .property(runtime, name)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn is_from_ast(&self, runtime: &baml_runtime::BamlRuntime) -> Result<bool, String> {
        self.inner.is_from_ast(runtime).map_err(|e| e.to_string())
    }
}

#[export_baml_fn]
impl ClassPropertyBuilderWrapper {
    #[export_baml_fn]
    fn name(&self) -> String {
        self.inner.property_name.to_string()
    }

    #[export_baml_fn]
    fn set_description(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        description: &str,
    ) -> Result<(), String> {
        self.inner
            .set_description(runtime, description)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn set_alias(&self, runtime: &baml_runtime::BamlRuntime, alias: &str) -> Result<(), String> {
        self.inner
            .set_alias(runtime, alias)
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn set_type(
        &self,
        runtime: &baml_runtime::BamlRuntime,
        field_type: &TypeWrapper,
    ) -> Result<(), String> {
        self.inner
            .set_type(runtime, field_type.as_ref().clone())
            .map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn description(&self, runtime: &baml_runtime::BamlRuntime) -> Result<Option<String>, String> {
        self.inner.description(runtime).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn alias(&self, runtime: &baml_runtime::BamlRuntime) -> Result<Option<String>, String> {
        self.inner.alias(runtime).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn type_(&self, runtime: &baml_runtime::BamlRuntime) -> Result<TypeIR, String> {
        self.inner.type_(runtime).map_err(|e| e.to_string())
    }

    #[export_baml_fn]
    fn is_from_ast(&self, runtime: &baml_runtime::BamlRuntime) -> Result<bool, String> {
        self.inner.is_from_ast(runtime).map_err(|e| e.to_string())
    }
}
