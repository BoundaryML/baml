use baml_types::BamlMap;

use crate::raw_ptr_wrapper::RawPtrType;

#[derive(Clone, Debug)]
pub enum ValueBase<T> {
    String(String, T),
    Int(i64, T),
    Float(f64, T),
    Bool(bool, T),
    Map(BamlMap<String, ValueBase<T>>, T),
    List(Vec<ValueBase<T>>, T),
    Null(T),
    RawPtr(RawPtrType, T),
    #[allow(dead_code)]
    Class(String, BamlMap<String, ValueBase<T>>, T),
    #[allow(dead_code)]
    Enum(String, String, T),
}

pub type Value = ValueBase<()>;
