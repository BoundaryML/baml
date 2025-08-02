use baml_cffi_macros::export_baml_fn;
use baml_types::{ir_type::UnionConstructor, BamlValue, TypeIR};

use super::{BamlObjectResponse, BamlObjectResponseSuccess, CallMethod};
use crate::raw_ptr_wrapper::{TypeBuilderWrapper, TypeWrapper};

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
}
