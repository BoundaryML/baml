use internal_baml_core::ir::Enum;

pub fn ir_enum_to_ts(enum_: &Enum) -> crate::generated_types::EnumTS {
    crate::generated_types::EnumTS {
        name: enum_.elem.name.clone(),
        values: enum_
            .elem
            .values
            .iter()
            .map(|(val, doc_string)| (val.elem.0.clone(), doc_string.as_ref().map(|d| d.0.clone())))
            .collect(),
        docstring: enum_.elem.docstring.as_ref().map(|d| d.0.clone()),
        dynamic: enum_.attributes.dynamic(),
    }
}
