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
    ReceiverParameter,
    ErrorLocal,
    CallbackErrorLocal,
    ResultLocal,
    ZeroLocal,
    ArgumentsLocal,
    TypeArgumentsLocal,
    CallbackOptionalCountLocal,
    OptionsParameter,
    OptionLocal,
    OptionValueParameter,
    ClassValueLocal,
    DecodedLocal,
    CodecValueParameter,
    UnionArmLocal,
    UnionOkLocal,
    UnionNullLocal,
    UnionSelectedLocal,
    UnionPayloadLocal,
    UnionVariantField,
    StringType,
    Int64Type,
    Float64Type,
    BoolType,
    ByteType,
    ErrorType,
    ClassNameMethod,
    InputMethod,
    ReflectedTypeInputMethod,
    ReflectedTypeOutputMethod,
    UnionArmMatchMethod,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClassCodecDirection {
    Encode,
    Decode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EnumCodecDirection {
    Encode,
    Decode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UnionCodecDirection {
    Encode,
    Decode,
}

/// A generator-owned package declaration. Projected BAML declarations are
/// exported and can never start with `_`, so this namespace cannot collide
/// with user code. The stable index comes from sorted BAML class FQNs.
pub(crate) struct ClassCodecIdent {
    direction: ClassCodecDirection,
    index: usize,
}

/// A generator-owned enum codec name. Stable indexes come from sorted BAML
/// enum FQNs independently of class codec indexes.
pub(crate) struct EnumCodecIdent {
    direction: EnumCodecDirection,
    index: usize,
}

pub(crate) struct UnionCodecIdent {
    direction: UnionCodecDirection,
    index: usize,
}

pub(crate) struct CallbackCodecIdent {
    index: usize,
}

pub(crate) struct CallbackArgumentIdent {
    index: usize,
}

impl CallbackArgumentIdent {
    pub(crate) fn new(index: usize) -> Self {
        Self { index }
    }
}

impl fmt::Display for CallbackArgumentIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "_bamlCallbackArg{}_", self.index)
    }
}

impl CallbackCodecIdent {
    pub(crate) fn new(index: usize) -> Self {
        Self { index }
    }
}

impl fmt::Display for CallbackCodecIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "_bamlEncodeCallback{}", self.index)
    }
}

impl UnionCodecIdent {
    pub(crate) fn new(direction: UnionCodecDirection, index: usize) -> Self {
        Self { direction, index }
    }
}

impl fmt::Display for UnionCodecIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let direction = match self.direction {
            UnionCodecDirection::Encode => "Encode",
            UnionCodecDirection::Decode => "Decode",
        };
        write!(f, "_baml{direction}Union{}", self.index)
    }
}

impl EnumCodecIdent {
    pub(crate) fn new(direction: EnumCodecDirection, index: usize) -> Self {
        Self { direction, index }
    }
}

impl ClassCodecIdent {
    pub(crate) fn new(direction: ClassCodecDirection, index: usize) -> Self {
        Self { direction, index }
    }
}

impl fmt::Display for ClassCodecIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let direction = match self.direction {
            ClassCodecDirection::Encode => "Encode",
            ClassCodecDirection::Decode => "Decode",
        };
        write!(f, "_baml{direction}Class{}", self.index)
    }
}

impl fmt::Display for EnumCodecIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let direction = match self.direction {
            EnumCodecDirection::Encode => "Encode",
            EnumCodecDirection::Decode => "Decode",
        };
        write!(f, "_baml{direction}Enum{}", self.index)
    }
}

impl GeneratorIdent {
    /// Every identifier owned by the generator. Package aliases and projected
    /// BAML locals both consult this same set, so a new renderer-owned name
    /// cannot silently introduce a shadowing bug in one of those scopes.
    pub(crate) const ALL: &'static [Self] = &[
        Self::ContextPackage,
        Self::BigPackage,
        Self::BootstrapPackage,
        Self::RuntimePackage,
        Self::ContextParameter,
        Self::ReceiverParameter,
        Self::ErrorLocal,
        Self::CallbackErrorLocal,
        Self::ResultLocal,
        Self::ZeroLocal,
        Self::ArgumentsLocal,
        Self::TypeArgumentsLocal,
        Self::CallbackOptionalCountLocal,
        Self::OptionsParameter,
        Self::OptionLocal,
        Self::OptionValueParameter,
        Self::ClassValueLocal,
        Self::DecodedLocal,
        Self::CodecValueParameter,
        Self::UnionArmLocal,
        Self::UnionOkLocal,
        Self::UnionNullLocal,
        Self::UnionSelectedLocal,
        Self::UnionPayloadLocal,
        Self::UnionVariantField,
        Self::StringType,
        Self::Int64Type,
        Self::Float64Type,
        Self::BoolType,
        Self::ByteType,
        Self::ErrorType,
        Self::ClassNameMethod,
        Self::InputMethod,
        Self::ReflectedTypeInputMethod,
        Self::ReflectedTypeOutputMethod,
        Self::UnionArmMatchMethod,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ContextPackage => "context",
            Self::BigPackage => "big",
            Self::BootstrapPackage => "bootstrap",
            Self::RuntimePackage => "baml_go",
            Self::ContextParameter => "ctx_",
            Self::ReceiverParameter => "receiver_",
            Self::ErrorLocal => "err_",
            Self::CallbackErrorLocal => "callbackErr_",
            Self::ResultLocal => "result_",
            Self::ZeroLocal => "zero_",
            Self::ArgumentsLocal => "arguments_",
            Self::TypeArgumentsLocal => "typeArguments_",
            Self::CallbackOptionalCountLocal => "optionalCount_",
            Self::OptionsParameter => "options_",
            Self::OptionLocal => "option_",
            Self::OptionValueParameter => "value_",
            Self::ClassValueLocal => "classValue_",
            Self::DecodedLocal => "decoded_",
            Self::CodecValueParameter => "value_",
            Self::UnionArmLocal => "arm_",
            Self::UnionOkLocal => "ok_",
            Self::UnionNullLocal => "null_",
            Self::UnionSelectedLocal => "selected_",
            Self::UnionPayloadLocal => "payload_",
            Self::UnionVariantField => "variant_",
            Self::StringType => "string",
            Self::Int64Type => "int64",
            Self::Float64Type => "float64",
            Self::BoolType => "bool",
            Self::ByteType => "byte",
            Self::ErrorType => "error",
            Self::ClassNameMethod => "BAMLClassName",
            Self::InputMethod => "BAMLInput",
            Self::ReflectedTypeInputMethod | Self::ReflectedTypeOutputMethod => "Type",
            Self::UnionArmMatchMethod => "MatchesUnionArm",
        }
    }
}

/// Non-keyword spellings that generated Go identifiers must not claim.
/// `nil` is predeclared, while `init` and `main` have package-level meaning.
pub(crate) fn is_protected_go_identifier(value: &str) -> bool {
    GeneratorIdent::ALL
        .iter()
        .any(|identifier| identifier.as_str() == value)
        || matches!(value, "nil" | "init" | "main")
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
    fn class_codec_identifiers_are_generator_owned_and_stable() {
        assert_eq!(
            ClassCodecIdent::new(ClassCodecDirection::Encode, 7).to_string(),
            "_bamlEncodeClass7"
        );
        assert_eq!(
            ClassCodecIdent::new(ClassCodecDirection::Decode, 7).to_string(),
            "_bamlDecodeClass7"
        );
        assert_eq!(
            EnumCodecIdent::new(EnumCodecDirection::Encode, 4).to_string(),
            "_bamlEncodeEnum4"
        );
        assert_eq!(
            EnumCodecIdent::new(EnumCodecDirection::Decode, 4).to_string(),
            "_bamlDecodeEnum4"
        );
    }

    #[test]
    fn callback_identifiers_are_generator_owned_and_stable() {
        assert_eq!(
            CallbackCodecIdent::new(3).to_string(),
            "_bamlEncodeCallback3"
        );
        assert_eq!(
            CallbackArgumentIdent::new(4).to_string(),
            "_bamlCallbackArg4_"
        );
        assert_eq!(
            GeneratorIdent::CallbackErrorLocal.to_string(),
            "callbackErr_"
        );
    }

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
