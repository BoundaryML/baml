//! The declared surface of a program, as a generated SDK ships it to its
//! bridge.
//!
//! A host encodes a value against the type the program declares for it. A
//! generated SDK is the only place a host learns those types, so the generator
//! bakes them in next to the wrappers it emits: every callable's signature and
//! every declaration a signature can reach. The bridge reads this table once
//! and walks host values against it; it never asks the engine what a function
//! expects.
//!
//! Names are keys into this table and nothing more. A bridge compares a name
//! with another name from the same table; it never inspects a name's spelling.
//! Types are the codegen family ([`Ty`]): aliases stay nominal, type variables
//! stay named, and nothing here depends on how the engine represents a type.
//!
//! The encoding is borsh, the same serialization the type family uses for
//! compiled artifacts. Producer and consumer are both Rust, so a wire-neutral
//! encoding would buy nothing; the first word is a format version so a stale
//! SDK fails on load rather than mid-call.

use std::collections::HashMap;

use baml_base::Name;
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{Function, Name as TypeName, ParamTy, Symbol, SymbolPool, Ty};

/// Bumped whenever the shape of [`ProgramDescriptor`] changes. A bridge
/// refuses a descriptor written by a generator at another version.
///
/// Version 2: a class carries its `crossing`.
pub const DESCRIPTOR_FORMAT_VERSION: u32 = 2;

/// Every callable and every declaration a generated SDK can mention.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProgramDescriptor {
    /// Sorted by `fqn`.
    pub functions: Vec<FunctionDescriptor>,
    /// Sorted by name.
    pub classes: Vec<ClassDescriptor>,
    /// Sorted by name.
    pub enums: Vec<EnumDescriptor>,
    /// Sorted by name.
    pub type_aliases: Vec<TypeAliasDescriptor>,
}

/// One callable the SDK can invoke by name: a free function, a static method
/// or an instance method.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FunctionDescriptor {
    /// The name the generated wrapper passes to the engine. A method's is its
    /// class's name followed by `.` and the method name.
    pub fqn: String,
    /// Type parameters of the enclosing class, for an instance or static
    /// method. They come first in De Bruijn order; a `TypeVar` in `params`
    /// with an index below their count refers to one of them.
    pub class_generic_params: Vec<Name>,
    /// The callable's own type parameters, after `class_generic_params`.
    pub generic_params: Vec<Name>,
    /// In declaration order. An instance method's receiver is the first
    /// entry, named `self`, typed as the class applied to its own parameters.
    pub params: Vec<ParamDescriptor>,
    pub ret: Ty,
    /// `Never` when the callable throws nothing.
    pub throws: Ty,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ParamDescriptor {
    pub name: Name,
    pub ty: Ty,
    /// The parameter has a default and may be omitted by the host.
    pub has_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ClassDescriptor {
    pub name: TypeName,
    pub generic_params: Vec<Name>,
    /// In declaration order. A field's type may mention `generic_params` as
    /// `TypeVar`s; the reader substitutes the instance's arguments.
    pub fields: Vec<FieldDescriptor>,
    /// How an instance crosses the host boundary.
    pub crossing: baml_type::ClassCrossing,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FieldDescriptor {
    pub name: Name,
    pub ty: Ty,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EnumDescriptor {
    pub name: TypeName,
    /// In declaration order. A variant's wire value is its name.
    pub variants: Vec<Name>,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TypeAliasDescriptor {
    pub name: TypeName,
    pub resolves_to: Ty,
    /// The body reaches the alias itself. A reader expanding aliases needs
    /// no cycle guard when this is false.
    pub recursive: bool,
}

/// Why a descriptor could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DescriptorError {
    #[error(
        "program descriptor format {found} is not the bridge's format {expected}; regenerate the SDK"
    )]
    FormatVersion { found: u32, expected: u32 },
    #[error("program descriptor is malformed: {0}")]
    Malformed(String),
}

impl ProgramDescriptor {
    /// Describe every symbol in `pool`.
    pub fn describe(pool: &SymbolPool) -> Self {
        let mut entries: Vec<(&TypeName, &Symbol)> = pool.iter().collect();
        entries.sort_by_key(|(name, _)| *name);

        let mut functions = Vec::new();
        let mut classes = Vec::new();
        let mut enums = Vec::new();
        let mut type_aliases = Vec::new();

        for (name, symbol) in entries {
            match symbol {
                Symbol::Function(function) => {
                    functions.push(describe_function(name.to_string(), &[], None, function));
                }
                Symbol::Class(class) => {
                    let class_fqn = name.to_string();
                    let receiver = Ty::Class(
                        name.clone(),
                        class
                            .generic_params
                            .iter()
                            .enumerate()
                            .map(|(index, param)| {
                                Ty::TypeVar(ParamTy::new(
                                    u32::try_from(index).expect("class generic count fits in u32"),
                                    param.clone(),
                                ))
                            })
                            .collect(),
                    );
                    for method in &class.static_methods {
                        functions.push(describe_function(
                            format!("{class_fqn}.{}", method.name.as_str()),
                            &class.generic_params,
                            None,
                            method,
                        ));
                    }
                    for method in &class.instance_methods {
                        functions.push(describe_function(
                            format!("{class_fqn}.{}", method.name.as_str()),
                            &class.generic_params,
                            Some(&receiver),
                            method,
                        ));
                    }
                    classes.push(ClassDescriptor {
                        name: name.clone(),
                        generic_params: class.generic_params.clone(),
                        fields: class
                            .properties
                            .iter()
                            .map(|property| FieldDescriptor {
                                name: property.name.clone(),
                                ty: property.ty.clone(),
                            })
                            .collect(),
                        crossing: class.crossing,
                    });
                }
                Symbol::Enum(enumeration) => enums.push(EnumDescriptor {
                    name: name.clone(),
                    variants: enumeration
                        .variants
                        .iter()
                        .map(|variant| variant.name.clone())
                        .collect(),
                }),
                Symbol::TypeAlias(alias) => type_aliases.push(TypeAliasDescriptor {
                    name: name.clone(),
                    resolves_to: alias.resolves_to.clone(),
                    recursive: alias.recursive,
                }),
            }
        }

        // Methods were pushed in class order; the table is keyed by `fqn`.
        functions.sort_by(|a, b| a.fqn.cmp(&b.fqn));
        Self {
            functions,
            classes,
            enums,
            type_aliases,
        }
    }

    /// Serialize with the format version first.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        DESCRIPTOR_FORMAT_VERSION
            .serialize(&mut bytes)
            .expect("writing to a Vec cannot fail");
        self.serialize(&mut bytes)
            .expect("writing to a Vec cannot fail");
        bytes
    }

    /// Read a descriptor written by [`Self::encode`] at this format version.
    pub fn decode(bytes: &[u8]) -> Result<Self, DescriptorError> {
        let mut rest = bytes;
        let found = u32::deserialize(&mut rest)
            .map_err(|error| DescriptorError::Malformed(error.to_string()))?;
        if found != DESCRIPTOR_FORMAT_VERSION {
            return Err(DescriptorError::FormatVersion {
                found,
                expected: DESCRIPTOR_FORMAT_VERSION,
            });
        }
        let descriptor = Self::deserialize(&mut rest)
            .map_err(|error| DescriptorError::Malformed(error.to_string()))?;
        if !rest.is_empty() {
            return Err(DescriptorError::Malformed(format!(
                "{} trailing bytes after the descriptor",
                rest.len()
            )));
        }
        Ok(descriptor)
    }

    /// [`Self::encode`] as the single-line ASCII payload generated SDKs embed
    /// in source, in the same form as the program bytecode
    /// ([`baml_artifact::encode_embedded`]).
    pub fn encode_embedded(&self) -> String {
        baml_artifact::encode_embedded(&self.encode())
    }

    /// Read a descriptor embedded by [`Self::encode_embedded`].
    pub fn decode_embedded(encoded: &[u8]) -> Result<Self, DescriptorError> {
        let bytes =
            baml_artifact::decode_embedded(baml_artifact::ArtifactKind::ProgramDescriptor, encoded)
                .map_err(|error| DescriptorError::Malformed(error.to_string()))?;
        Self::decode(&bytes)
    }

    /// The callable registered under `fqn`, by binary search over the sorted
    /// table.
    pub fn function(&self, fqn: &str) -> Option<&FunctionDescriptor> {
        self.functions
            .binary_search_by(|function| function.fqn.as_str().cmp(fqn))
            .ok()
            .map(|index| &self.functions[index])
    }

    /// Index the declarations by name for repeated lookup.
    pub fn declarations(&self) -> Declarations<'_> {
        Declarations {
            classes: self
                .classes
                .iter()
                .map(|class| (&class.name, class))
                .collect(),
            enums: self.enums.iter().map(|e| (&e.name, e)).collect(),
            type_aliases: self
                .type_aliases
                .iter()
                .map(|alias| (&alias.name, alias))
                .collect(),
        }
    }
}

/// Name-keyed views over a descriptor's declarations.
#[derive(Debug)]
pub struct Declarations<'a> {
    pub classes: HashMap<&'a TypeName, &'a ClassDescriptor>,
    pub enums: HashMap<&'a TypeName, &'a EnumDescriptor>,
    pub type_aliases: HashMap<&'a TypeName, &'a TypeAliasDescriptor>,
}

fn describe_function(
    fqn: String,
    class_generic_params: &[Name],
    receiver: Option<&Ty>,
    function: &Function,
) -> FunctionDescriptor {
    let mut params = Vec::with_capacity(function.arguments.len() + usize::from(receiver.is_some()));
    if let Some(receiver) = receiver {
        params.push(ParamDescriptor {
            name: Name::new("self"),
            ty: receiver.clone(),
            has_default: false,
        });
    }
    params.extend(function.arguments.iter().map(|argument| ParamDescriptor {
        name: argument.name.clone(),
        ty: argument.ty.clone(),
        has_default: argument.default.is_some(),
    }));
    FunctionDescriptor {
        fqn,
        class_generic_params: class_generic_params.to_vec(),
        generic_params: function.generic_params.clone(),
        params,
        ret: function.return_type.clone(),
        throws: function.throws.clone().unwrap_or(Ty::Never),
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;

    use super::*;
    use crate::{
        Class, ClassProperty, Enum, EnumVariant, FunctionArgument, FunctionArgumentDefault, Origin,
        TypeAlias,
    };

    fn type_name(value: &str) -> TypeName {
        TypeName::new(BaseName::new("user"), Vec::new(), BaseName::new(value))
    }

    fn origin() -> Origin {
        Origin {
            source_file_path: "types.baml".to_string(),
            span_start: 0,
        }
    }

    fn argument(name: &str, ty: Ty, default: Option<FunctionArgumentDefault>) -> FunctionArgument {
        FunctionArgument {
            name: BaseName::new(name),
            docstring: None,
            ty,
            default,
            injected: false,
        }
    }

    fn function(name: &str, generic_params: &[&str], arguments: Vec<FunctionArgument>) -> Function {
        Function {
            name: BaseName::new(name),
            generic_params: generic_params.iter().copied().map(BaseName::new).collect(),
            docstring: None,
            arguments,
            return_type: Ty::String,
            throws: None,
            watchers: Vec::new(),
            origin: origin(),
        }
    }

    fn pool() -> SymbolPool {
        let mut pool = SymbolPool::new();
        pool.insert(
            type_name("greet"),
            Symbol::Function(function(
                "greet",
                &[],
                vec![
                    argument("who", Ty::String, None),
                    argument(
                        "times",
                        Ty::Int,
                        Some(FunctionArgumentDefault::Literal(
                            crate::DefaultLiteral::Scalar(baml_base::Literal::Int(1)),
                        )),
                    ),
                ],
            )),
        );
        pool.insert(
            type_name("Box"),
            Symbol::Class(Class {
                crossing: crate::ClassCrossing::Fields,
                name: type_name("Box"),
                generic_params: vec![BaseName::new("T")],
                docstring: None,
                properties: vec![ClassProperty {
                    name: BaseName::new("item"),
                    docstring: None,
                    ty: Ty::TypeVar(ParamTy::new(0, BaseName::new("T"))),
                }],
                static_methods: vec![function("empty", &[], vec![])],
                instance_methods: vec![function(
                    "map",
                    &["U"],
                    vec![argument(
                        "f",
                        Ty::Function {
                            params: Box::new([crate::CallableParam::required(
                                None,
                                Ty::TypeVar(ParamTy::new(0, BaseName::new("T"))),
                            )]),
                            ret: Box::new(Ty::TypeVar(ParamTy::new(1, BaseName::new("U")))),
                            throws: Box::new(Ty::Never),
                        },
                        None,
                    )],
                )],
                origin: origin(),
            }),
        );
        pool.insert(
            type_name("Mood"),
            Symbol::Enum(Enum {
                name: type_name("Mood"),
                docstring: None,
                variants: ["Happy", "Sad"]
                    .iter()
                    .map(|v| EnumVariant {
                        name: BaseName::new(v),
                        docstring: None,
                        value: (*v).to_string(),
                    })
                    .collect(),
                origin: origin(),
            }),
        );
        pool.insert(
            type_name("Tree"),
            Symbol::TypeAlias(TypeAlias {
                name: type_name("Tree"),
                resolves_to: Ty::Union(Box::new([
                    Ty::Int,
                    Ty::List(Box::new(Ty::TypeAlias(type_name("Tree")))),
                ])),
                recursive: true,
                origin: origin(),
            }),
        );
        pool
    }

    #[test]
    fn describes_every_symbol_kind_in_name_order() {
        let descriptor = ProgramDescriptor::describe(&pool());

        let fqns: Vec<&str> = descriptor
            .functions
            .iter()
            .map(|f| f.fqn.as_str())
            .collect();
        assert_eq!(fqns, ["user.Box.empty", "user.Box.map", "user.greet"]);

        let greet = descriptor.function("user.greet").unwrap();
        assert_eq!(greet.params.len(), 2);
        assert!(!greet.params[0].has_default);
        assert!(greet.params[1].has_default);
        assert_eq!(greet.throws, Ty::Never);

        let map = descriptor.function("user.Box.map").unwrap();
        assert_eq!(map.class_generic_params, vec![BaseName::new("T")]);
        assert_eq!(map.generic_params, vec![BaseName::new("U")]);
        assert_eq!(map.params[0].name, BaseName::new("self"));
        assert_eq!(
            map.params[0].ty,
            Ty::Class(
                type_name("Box"),
                Box::new([Ty::TypeVar(ParamTy::new(0, BaseName::new("T")))])
            )
        );
        assert_eq!(map.params[1].name, BaseName::new("f"));

        let empty = descriptor.function("user.Box.empty").unwrap();
        assert!(empty.params.is_empty(), "a static method has no receiver");
        assert_eq!(empty.class_generic_params, vec![BaseName::new("T")]);

        let declarations = descriptor.declarations();
        let class = declarations.classes[&type_name("Box")];
        assert_eq!(class.fields[0].name, BaseName::new("item"));
        let enumeration = declarations.enums[&type_name("Mood")];
        assert_eq!(enumeration.variants.len(), 2);
        let alias = declarations.type_aliases[&type_name("Tree")];
        assert!(alias.recursive);
        assert!(descriptor.function("user.nope").is_none());
    }

    #[test]
    fn encoding_round_trips_and_is_deterministic() {
        let descriptor = ProgramDescriptor::describe(&pool());
        let bytes = descriptor.encode();
        assert_eq!(bytes, ProgramDescriptor::describe(&pool()).encode());
        assert_eq!(ProgramDescriptor::decode(&bytes).unwrap(), descriptor);

        let embedded = descriptor.encode_embedded();
        assert!(embedded.is_ascii() && !embedded.contains(['"', '\\', '\n']));
        assert_eq!(
            ProgramDescriptor::decode_embedded(embedded.as_bytes()).unwrap(),
            descriptor
        );
        assert!(matches!(
            ProgramDescriptor::decode_embedded(b"not base64!"),
            Err(DescriptorError::Malformed(_))
        ));
    }

    #[test]
    fn decode_rejects_another_format_version_and_trailing_bytes() {
        let descriptor = ProgramDescriptor::describe(&pool());
        let mut bytes = descriptor.encode();

        let mut other_version = bytes.clone();
        other_version[..4].copy_from_slice(&(DESCRIPTOR_FORMAT_VERSION + 1).to_le_bytes());
        assert_eq!(
            ProgramDescriptor::decode(&other_version),
            Err(DescriptorError::FormatVersion {
                found: DESCRIPTOR_FORMAT_VERSION + 1,
                expected: DESCRIPTOR_FORMAT_VERSION,
            })
        );

        bytes.push(0);
        assert!(matches!(
            ProgramDescriptor::decode(&bytes),
            Err(DescriptorError::Malformed(message)) if message.contains("trailing")
        ));

        assert!(matches!(
            ProgramDescriptor::decode(&[1, 0]),
            Err(DescriptorError::Malformed(_))
        ));
    }
}
