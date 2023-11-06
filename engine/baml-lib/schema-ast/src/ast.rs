mod argument;
mod attribute;
mod r#class;
mod client;
mod comment;
mod config;
mod configurations;
mod r#enum;
mod expression;
mod field;
mod find_at_position;
mod function;
mod generator_config;
mod identifier;
mod indentation_type;
mod newline_type;
mod printer_config;
mod retry_policy_config;
mod serializer;
mod top;
mod traits;
mod variant;

pub(crate) use self::comment::Comment;

pub use argument::{ArguementId, Argument, ArgumentsList};
pub use attribute::{Attribute, AttributeContainer, AttributeId};
pub use client::Client;
pub use config::ConfigBlockProperty;
pub use configurations::Configuration;
pub use expression::Expression;
pub use field::{Field, FieldArity, FieldType};
pub use find_at_position::*;
pub use function::{FuncArguementId, Function, FunctionArg, FunctionArgs, NamedFunctionArgList};
pub use generator_config::GeneratorConfig;
pub use identifier::{Identifier, RefIdentifier, TypeValue};
pub use indentation_type::IndentationType;
pub use internal_baml_diagnostics::Span;
pub use newline_type::NewlineType;
pub use printer_config::PrinterConfig;
pub use r#class::{Class, FieldId};
pub use r#enum::{Enum, EnumValue, EnumValueId};
pub use retry_policy_config::RetryPolicyConfig;
pub use serializer::{Serializer, SerializerField, SerializerFieldId};
pub use top::Top;
pub use traits::{WithAttributes, WithDocumentation, WithIdentifier, WithName, WithSpan};
pub use variant::{FieldId as VariantFieldId, SerializerId as VariantSerializerId, Variant};

/// AST representation of a prisma schema.
///
/// This module is used internally to represent an AST. The AST's nodes can be used
/// during validation of a schema, especially when implementing custom attributes.
///
/// The AST is not validated, also fields and attributes are not resolved. Every node is
/// annotated with its location in the text representation.
/// Basically, the AST is an object oriented representation of the datamodel's text.
/// Schema = Datamodel + Generators + Datasources
#[derive(Debug)]
pub struct SchemaAst {
    /// All models, enums, composite types, datasources, generators and type aliases.
    pub tops: Vec<Top>,
}

impl SchemaAst {
    pub fn new() -> Self {
        SchemaAst { tops: Vec::new() }
    }

    /// Iterate over all the top-level items in the schema.
    pub fn iter_tops(&self) -> impl Iterator<Item = (TopId, &Top)> {
        self.tops
            .iter()
            .enumerate()
            .map(|(top_idx, top)| (top_idx_to_top_id(top_idx, top), top))
    }

    /// Iterate over all the generator blocks in the schema.
    pub fn generators(&self) -> impl Iterator<Item = &GeneratorConfig> {
        self.tops.iter().filter_map(|top| top.as_generator())
    }
}

/// An opaque identifier for an enum in a schema AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnumId(u32);
impl std::ops::Index<EnumId> for SchemaAst {
    type Output = Enum;

    fn index(&self, index: EnumId) -> &Self::Output {
        self.tops[index.0 as usize].as_enum().unwrap()
    }
}

/// An opaque identifier for a model in a schema AST. Use the
/// `schema[model_id]` syntax to resolve the id to an `ast::Model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassId(u32);
impl std::ops::Index<ClassId> for SchemaAst {
    type Output = Class;

    fn index(&self, index: ClassId) -> &Self::Output {
        self.tops[index.0 as usize].as_class().unwrap()
    }
}

/// An opaque identifier for a model in a schema AST. Use the
/// `schema[model_id]` syntax to resolve the id to an `ast::Model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionId(u32);
impl std::ops::Index<FunctionId> for SchemaAst {
    type Output = Function;

    fn index(&self, index: FunctionId) -> &Self::Output {
        self.tops[index.0 as usize].as_function().unwrap()
    }
}

/// An opaque identifier for a model in a schema AST. Use the
/// `schema[model_id]` syntax to resolve the id to an `ast::Model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientId(u32);
impl std::ops::Index<ClientId> for SchemaAst {
    type Output = Client;

    fn index(&self, index: ClientId) -> &Self::Output {
        self.tops[index.0 as usize].as_client().unwrap()
    }
}

/// An opaque identifier for a model in a schema AST. Use the
/// `schema[model_id]` syntax to resolve the id to an `ast::Model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GeneratorConfigId(u32);
impl std::ops::Index<GeneratorConfigId> for SchemaAst {
    type Output = GeneratorConfig;

    fn index(&self, index: GeneratorConfigId) -> &Self::Output {
        self.tops[index.0 as usize].as_generator().unwrap()
    }
}

/// An opaque identifier for a model in a schema AST. Use the
/// `schema[model_id]` syntax to resolve the id to an `ast::Model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariantConfigId(u32);
impl std::ops::Index<VariantConfigId> for SchemaAst {
    type Output = Variant;

    fn index(&self, index: VariantConfigId) -> &Self::Output {
        self.tops[index.0 as usize].as_variant().unwrap()
    }
}

/// An opaque identifier for a model in a schema AST. Use the
/// `schema[model_id]` syntax to resolve the id to an `ast::Model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConfigurationId(u32);
impl std::ops::Index<ConfigurationId> for SchemaAst {
    type Output = Configuration;

    fn index(&self, index: ConfigurationId) -> &Self::Output {
        self.tops[index.0 as usize].as_configurations().unwrap()
    }
}

/// An identifier for a top-level item in a schema AST. Use the `schema[top_id]`
/// syntax to resolve the id to an `ast::Top`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TopId {
    /// An enum declaration
    Enum(EnumId),

    // A class declaration
    Class(ClassId),

    // A function declaration
    Function(FunctionId),

    // A client declaration
    Client(ClientId),

    // A generator declaration
    Generator(GeneratorConfigId),

    // A variant declaration
    Variant(VariantConfigId),

    // A config block
    Config((ConfigurationId, &'static str)),
}

impl TopId {
    pub fn as_variant_id(self) -> Option<VariantConfigId> {
        match self {
            TopId::Variant(id) => Some(id),
            _ => None,
        }
    }

    /// Try to interpret the top as an enum.
    pub fn as_enum_id(self) -> Option<EnumId> {
        match self {
            TopId::Enum(id) => Some(id),
            _ => None,
        }
    }

    /// Try to interpret the top as a class.
    pub fn as_class_id(self) -> Option<ClassId> {
        match self {
            TopId::Class(id) => Some(id),
            _ => None,
        }
    }

    /// Try to interpret the top as a function.
    pub fn as_function_id(self) -> Option<FunctionId> {
        match self {
            TopId::Function(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_client_id(self) -> Option<ClientId> {
        match self {
            TopId::Client(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_retry_policy_id(self) -> Option<ConfigurationId> {
        match self {
            TopId::Config((id, "retry_policy")) => Some(id),
            _ => None,
        }
    }

    pub fn as_printer_id(self) -> Option<ConfigurationId> {
        match self {
            TopId::Config((id, "printer")) => Some(id),
            _ => None,
        }
    }
}

impl std::ops::Index<TopId> for SchemaAst {
    type Output = Top;

    fn index(&self, index: TopId) -> &Self::Output {
        let idx = match index {
            TopId::Enum(EnumId(idx)) => idx,
            TopId::Class(ClassId(idx)) => idx,
            TopId::Function(FunctionId(idx)) => idx,
            TopId::Client(ClientId(idx)) => idx,
            TopId::Generator(GeneratorConfigId(idx)) => idx,
            TopId::Variant(VariantConfigId(idx)) => idx,
            TopId::Config((ConfigurationId(idx), _)) => idx,
        };

        &self.tops[idx as usize]
    }
}

fn top_idx_to_top_id(top_idx: usize, top: &Top) -> TopId {
    match top {
        Top::Enum(_) => TopId::Enum(EnumId(top_idx as u32)),
        Top::Class(_) => TopId::Class(ClassId(top_idx as u32)),
        Top::Function(_) => TopId::Function(FunctionId(top_idx as u32)),
        Top::Client(_) => TopId::Client(ClientId(top_idx as u32)),
        Top::Generator(_) => TopId::Generator(GeneratorConfigId(top_idx as u32)),
        Top::Variant(_) => TopId::Variant(VariantConfigId(top_idx as u32)),
        Top::Config(c) => TopId::Config((ConfigurationId(top_idx as u32), c.get_type())),
    }
}
