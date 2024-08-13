use crate::dir_writer::LanguageFeatures;
use baml_types::{BamlMediaType, TypeValue};

#[derive(Default)]
pub(super) struct TypescriptLanguageFeatures {}

impl LanguageFeatures for TypescriptLanguageFeatures {
    const CONTENT_PREFIX: &'static str = r#"
/*************************************************************************************************

Welcome to Baml! To use this generated code, please run one of the following:

$ npm install @boundaryml/baml
$ yarn add @boundaryml/baml
$ pnpm add @boundaryml/baml

*************************************************************************************************/

// This file was generated by BAML: do not edit it. Instead, edit the BAML
// files and re-generate this code.
//
// tslint:disable
// @ts-nocheck
// biome-ignore format: autogenerated code
/* eslint-disable */
        "#;
}

pub(super) trait ToTypescript {
    fn to_typescript(&self) -> String;
}

impl ToTypescript for TypeValue {
    fn to_typescript(&self) -> String {
        let var_name = &match self {
            TypeValue::Bool => "boolean",
            TypeValue::Float => "number",
            TypeValue::Int => "number",
            TypeValue::String => "string",
            TypeValue::Null => "null",
            TypeValue::Media(BamlMediaType::Image) => "Image",
            TypeValue::Media(BamlMediaType::Audio) => "Audio",
        };
        var_name.to_string()
    }
}
