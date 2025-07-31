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
    Enum(String, String),
    Class(String, BamlMap<String, ValueBase<T>>, T),
    Null(T),
    RawPtr(RawPtrType, T),
}

pub type Value = ValueBase<()>;
