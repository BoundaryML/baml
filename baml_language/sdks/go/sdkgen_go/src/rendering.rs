//! Generator-owned Go identifiers.
//!
//! These names share lexical scope with projected BAML parameters. Keeping
//! them typed and centralized lets name allocation reserve exactly the names
//! the renderer emits.

use std::{collections::BTreeMap, fmt, fmt::Write as _};

use crate::names::GoPackageName;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GeneratorIdent {
    ContextPackage,
    BigPackage,
    BootstrapPackage,
    RuntimePackage,
    ContextParameter,
    ErrorLocal,
    ResultLocal,
    ZeroLocal,
    WireValueParameter,
    ClassValueLocal,
    DecodedLocal,
    StringType,
    Int64Type,
    Float64Type,
    BoolType,
    ByteType,
    ErrorType,
}

impl GeneratorIdent {
    pub(crate) const IMPORT_ALIASES: &'static [Self] = &[
        Self::ContextPackage,
        Self::BigPackage,
        Self::BootstrapPackage,
        Self::RuntimePackage,
    ];

    pub(crate) const FUNCTION_SCOPE: &'static [Self] = &[
        Self::ContextPackage,
        Self::BigPackage,
        Self::BootstrapPackage,
        Self::RuntimePackage,
        Self::ContextParameter,
        Self::ErrorLocal,
        Self::ResultLocal,
        Self::ZeroLocal,
        Self::StringType,
        Self::Int64Type,
        Self::Float64Type,
        Self::BoolType,
        Self::ByteType,
        Self::ErrorType,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ContextPackage => "context",
            Self::BigPackage => "big",
            Self::BootstrapPackage => "bootstrap",
            Self::RuntimePackage => "baml_go",
            Self::ContextParameter => "ctx",
            Self::ErrorLocal => "err",
            Self::ResultLocal => "result",
            Self::ZeroLocal => "zero",
            Self::WireValueParameter => "value",
            Self::ClassValueLocal => "classValue",
            Self::DecodedLocal => "decoded",
            Self::StringType => "string",
            Self::Int64Type => "int64",
            Self::Float64Type => "float64",
            Self::BoolType => "bool",
            Self::ByteType => "byte",
            Self::ErrorType => "error",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ImportSpec {
    path: Box<str>,
    explicit_alias: bool,
}

/// File-scoped import allocation keyed by the identifier visible to Go code.
#[derive(Default)]
pub(crate) struct GoImports {
    imports: BTreeMap<Box<str>, ImportSpec>,
}

impl GoImports {
    pub(crate) fn add_generator(&mut self, alias: GeneratorIdent, path: &str) {
        self.add(alias.as_str(), path, false);
    }

    pub(crate) fn add_package(&mut self, package: &GoPackageName, path: &str) {
        self.add(package.as_str(), path, true);
    }

    fn add(&mut self, alias: &str, path: &str, explicit_alias: bool) {
        let spec = ImportSpec {
            path: path.into(),
            explicit_alias,
        };
        if let Some(existing) = self.imports.insert(alias.into(), spec.clone()) {
            assert_eq!(existing, spec, "Go import alias mapped to multiple paths");
        }
    }

    pub(crate) fn render(&self) -> String {
        if self.imports.is_empty() {
            return String::new();
        }
        let mut out = String::from("import (\n");
        for (alias, spec) in &self.imports {
            if spec.explicit_alias {
                let _ = writeln!(out, "\t{alias} {:?}", spec.path);
            } else {
                let _ = writeln!(out, "\t{:?}", spec.path);
            }
        }
        out.push_str(")\n\n");
        out
    }
}

impl fmt::Display for GeneratorIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_are_deduplicated_and_alias_generated_packages_explicitly() {
        let mut imports = GoImports::default();
        imports.add_generator(GeneratorIdent::ContextPackage, "context");
        imports.add_generator(GeneratorIdent::ContextPackage, "context");
        imports.add_package(
            &GoPackageName::new("models"),
            "example.com/sdk/packages/models",
        );

        let rendered = imports.render();
        assert_eq!(rendered.matches("\"context\"").count(), 1);
        assert!(rendered.contains("models \"example.com/sdk/packages/models\""));
    }

    #[test]
    #[should_panic(expected = "Go import alias mapped to multiple paths")]
    fn one_alias_cannot_refer_to_two_packages() {
        let mut imports = GoImports::default();
        let alias = GoPackageName::new("models");
        imports.add_package(&alias, "example.com/one/models");
        imports.add_package(&alias, "example.com/two/models");
    }
}
