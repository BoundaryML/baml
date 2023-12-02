use internal_baml_schema_ast::ast::{Identifier, TypeValue, WithName};

use super::{
    file::escaped_string,
    traits::{WithToCode, WithToObject},
};

impl WithToObject for Identifier {
    fn to_py_object(&self, f: &mut super::file::File) -> String {
        match self {
            Identifier::ENV(s, _) => {
                f.add_import("os", "environ");
                format!("environ['{}']", s.clone())
            }
            _ => format!("\"{}\"", escaped_string(self.name(), ("\"", "\\\""))),
        }
    }

    fn to_ts_object(&self, f: &mut super::file::File) -> String {
        match self {
            Identifier::ENV(s, _) => {
                format!("process.env.{}", s.clone())
            }
            _ => format!("\"{}\"", escaped_string(self.name(), ("\"", "\\\""))),
        }
    }
}

impl WithToCode for Identifier {
    fn to_py_string(&self, f: &mut super::file::File) -> String {
        match self {
            Identifier::ENV(str, _) => {
                f.add_import("os", "environ");
                format!("environ['{}']", str)
            }
            Identifier::Ref(idn, _) => {
                f.add_import(&idn.path.join("."), &idn.name);
                idn.name.clone()
            }
            Identifier::Local(idn, _) => idn.into(),
            Identifier::String(str, _) => str.into(),
            Identifier::Primitive(p, _) => match p {
                TypeValue::Bool => "bool",
                TypeValue::Int => "int",
                TypeValue::Float => "float",
                TypeValue::Char => "str",
                TypeValue::String => "str",
                TypeValue::Null => "None",
            }
            .into(),
            Identifier::Invalid(inv, _) => panic!("Should never show invalid: {}", inv),
        }
    }

    fn to_ts_string(&self, f: &mut super::file::File) -> String {
        match self {
            Identifier::ENV(idn, _) => {
                format!("process.env.{}", idn)
            }
            Identifier::Ref(idn, _) => {
                f.add_import(&idn.path.join("."), &idn.name);
                idn.name.clone()
            }
            Identifier::Local(idn, _) => idn.into(),
            Identifier::String(str, _) => str.into(),
            Identifier::Primitive(p, _) => match p {
                TypeValue::Bool => "boolean",
                TypeValue::Int => "number",
                TypeValue::Float => "number",
                TypeValue::Char => "string",
                TypeValue::String => "string",
                TypeValue::Null => "null",
            }
            .into(),
            Identifier::Invalid(inv, _) => panic!("Should never show invalid: {}", inv),
        }
    }
}
