use std::collections::HashSet;

use crate::{CodegenTypeError, Ty};

pub type SymbolPool = std::collections::HashMap<super::Name, Symbol>;

#[derive(Clone)]
pub enum Symbol {
    Function(Function),
    Class(Class),
    Enum(Enum),
    TypeAlias(TypeAlias),
}

/// Where the symbol was defined in user BAML source.
#[derive(Clone, Debug)]
pub struct Origin {
    /// Path of the defining `.baml` file, relative to the
    /// `baml_src/` root. Same string form as keys in
    /// `_inlinedbaml.py`'s FILES dict.
    pub source_file_path: String,
    /// Byte offset of the symbol's definition start within that
    /// file. Used as the secondary sort key when ordering symbols
    /// inside an emitter leaf.
    pub span_start: u32,
}

#[derive(Clone)]
pub struct Function {
    pub name: baml_base::Name,
    /// `TypeVar`s declared on this function. Empty for non-generic functions.
    /// Mirrors AST `FunctionDef.generic_params`. Inner `Ty::TypeVar`
    /// references in `arguments` / `return_type` resolve against this list.
    pub generic_params: Vec<baml_base::Name>,
    pub docstring: Option<String>,
    pub arguments: Vec<FunctionArgument>,
    pub return_type: super::Ty,

    /// The function's inferred throws contract as a resolved `Ty`, or `None`
    /// when the function throws nothing (`callable_throws` → `Never`). A
    /// declared `throws` clause, when present, wins over inference; otherwise
    /// this is the inferred escaping-throws set. A `throws A | B` resolves to
    /// `Ty::Union([A, B])`. Generators derive the unqualified leaf names from
    /// this for the `Raises:` docstring block (32d).
    pub throws: Option<super::Ty>,

    // TODO: add other APIs here that impact code-gen
    pub watchers: Vec<(baml_base::Name, super::Ty)>,

    /// Source-origin info: the defining `.baml` file and byte span start.
    /// Used by the emitter to order symbols deterministically within a leaf.
    pub origin: Origin,
}

#[derive(Clone)]
pub struct FunctionArgument {
    pub name: baml_base::Name,
    pub docstring: Option<String>,
    pub ty: super::Ty,
    pub default: Option<FunctionArgumentDefault>,
    /// True for compiler-injected parameters (`on_event` on LLM functions and
    /// their `@stream` companions; the injected `client` never reaches the
    /// pool). Generators that cannot represent an injected parameter's type
    /// may omit it — its default fills in at the VM boundary — but must never
    /// drop a user-declared parameter, whatever its name or shape.
    pub injected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FunctionArgumentDefault {
    Null,
    Literal(DefaultLiteral),
    Expression { source: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DefaultLiteral {
    Scalar(baml_base::Literal),
    EmptyList,
    EmptyMap,
}

#[derive(Clone)]
pub struct Class {
    pub name: super::Name,
    /// `TypeVar`s declared on this class. Empty for non-generic classes.
    /// Mirrors AST `ClassDef.generic_params`. Inner `Ty::TypeVar`
    /// references in `properties` / methods resolve against this list.
    pub generic_params: Vec<baml_base::Name>,
    pub docstring: Option<String>,
    pub properties: Vec<ClassProperty>,
    /// Static methods on this class. Source-declaration order is
    /// preserved by the pool builder; the emitter sorts at fan-out
    /// time. Static vs. instance is encoded structurally — kind is
    /// implied by which vec the method lives in.
    pub static_methods: Vec<Function>,
    /// Instance methods on this class. The receiver (`self`) is **not**
    /// in `Function::arguments` — it's a Python convention prepended at
    /// render time.
    pub instance_methods: Vec<Function>,
    pub origin: Origin,
}

#[derive(Clone)]
pub struct ClassProperty {
    pub name: baml_base::Name,
    pub docstring: Option<String>,
    pub ty: super::Ty,
}

#[derive(Clone)]
pub struct Enum {
    pub name: super::Name,
    pub docstring: Option<String>,
    pub variants: Vec<EnumVariant>,
    pub origin: Origin,
}

#[derive(Clone)]
pub struct EnumVariant {
    pub name: baml_base::Name,
    pub docstring: Option<String>,
    pub value: String,
}

#[derive(Clone)]
pub struct TypeAlias {
    pub name: super::Name,
    pub resolves_to: super::Ty,
    /// Whether this type alias is recursive (i.e., references itself).
    pub recursive: bool,
    pub origin: Origin,
}

impl Symbol {
    pub fn walk_all_unions(&self) -> HashSet<super::Ty> {
        match self {
            Symbol::Function(function) => function.walk_all_unions(),
            Symbol::Class(class) => class.walk_all_unions(),
            Symbol::Enum(_) => HashSet::default(),
            Symbol::TypeAlias(type_alias) => type_alias.walk_all_unions(),
        }
    }
}

/// Validate map keys throughout a complete generator-facing symbol pool.
///
/// This is the authoritative alias-aware check: it follows canonical alias
/// declaration targets by fully-qualified name, rejects missing or cyclic
/// targets, and accepts only the string-denoting types supported by BAML.
pub fn validate_symbol_pool_map_keys(pool: &SymbolPool) -> Result<(), CodegenTypeError> {
    for symbol in pool.values() {
        match symbol {
            Symbol::Function(function) => function.validate_map_keys(pool)?,
            Symbol::Class(class) => class.validate_map_keys(pool)?,
            Symbol::Enum(_) => {}
            Symbol::TypeAlias(alias) => {
                validate_map_keys_ty(&alias.resolves_to, pool, &mut HashSet::new())?;
            }
        }
    }
    Ok(())
}

impl Function {
    fn walk_all_unions(&self) -> HashSet<super::Ty> {
        self.arguments
            .iter()
            .flat_map(|args| args.ty.walk_all_unions().into_iter())
            .chain(self.return_type.walk_all_unions())
            .collect()
    }

    fn validate_map_keys(&self, pool: &SymbolPool) -> Result<(), CodegenTypeError> {
        for argument in &self.arguments {
            validate_map_keys_ty(&argument.ty, pool, &mut HashSet::new())?;
        }
        validate_map_keys_ty(&self.return_type, pool, &mut HashSet::new())?;
        if let Some(throws) = &self.throws {
            validate_map_keys_ty(throws, pool, &mut HashSet::new())?;
        }
        for (_, watcher) in &self.watchers {
            validate_map_keys_ty(watcher, pool, &mut HashSet::new())?;
        }
        Ok(())
    }
}

impl Class {
    fn walk_all_unions(&self) -> HashSet<super::Ty> {
        self.properties
            .iter()
            .flat_map(|prop| prop.ty.walk_all_unions().into_iter())
            .chain(
                self.static_methods
                    .iter()
                    .chain(&self.instance_methods)
                    .flat_map(|m| m.walk_all_unions().into_iter()),
            )
            .collect::<_>()
    }

    fn validate_map_keys(&self, pool: &SymbolPool) -> Result<(), CodegenTypeError> {
        for property in &self.properties {
            validate_map_keys_ty(&property.ty, pool, &mut HashSet::new())?;
        }
        for method in self.static_methods.iter().chain(&self.instance_methods) {
            method.validate_map_keys(pool)?;
        }
        Ok(())
    }
}

impl TypeAlias {
    fn walk_all_unions(&self) -> HashSet<super::Ty> {
        self.resolves_to.walk_all_unions()
    }
}

trait WalkAllUnions {
    fn walk_all_unions(&self) -> HashSet<Ty>;
}

impl WalkAllUnions for Ty {
    fn walk_all_unions(&self) -> HashSet<Ty> {
        let mut unions = HashSet::default();
        if matches!(self, Ty::Union(..)) {
            unions.insert(self.clone());
        }

        match self {
            Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Void { .. }
            | Ty::Uint8Array { .. }
            | Ty::Media(..)
            | Ty::Enum(..)
            | Ty::EnumVariant(..)
            | Ty::TypeAlias(..)
            | Ty::TypeVar(..)
            | Ty::RustType { .. }
            | Ty::Type { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. }
            | Ty::Unknown { .. }
            | Ty::Never { .. }
            | Ty::Literal(..) => {}
            Ty::Class(_, args, _) => {
                for arg in args {
                    unions.extend(arg.walk_all_unions());
                }
            }
            Ty::Interface(_, generics, associated_types, _) => {
                for generic in generics {
                    unions.extend(generic.walk_all_unions());
                }
                for (_, ty) in associated_types {
                    unions.extend(ty.walk_all_unions());
                }
            }
            Ty::List(ty, _) => unions.extend(ty.walk_all_unions()),
            Ty::Map { key, value, .. } => {
                unions.extend(key.walk_all_unions());
                unions.extend(value.walk_all_unions());
            }
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                for param in params {
                    unions.extend(param.ty.walk_all_unions());
                }
                unions.extend(ret.walk_all_unions());
                unions.extend(throws.walk_all_unions());
            }
            Ty::Future(value, error, _) => {
                unions.extend(value.walk_all_unions());
                unions.extend(error.walk_all_unions());
            }
            // Codegen types are canonical at the compiler boundary, but keep
            // this public symbol traversal total for manually assembled pools.
            Ty::Union(members, _) => {
                for member in members {
                    unions.extend(member.walk_all_unions());
                }
            }
        }

        unions
    }
}

fn validate_map_keys_ty(
    ty: &Ty,
    pool: &SymbolPool,
    resolving_aliases: &mut HashSet<super::Name>,
) -> Result<(), CodegenTypeError> {
    match ty {
        Ty::Map { key, value, .. } => {
            if !map_key_resolves_to_string(key, pool, resolving_aliases) {
                return Err(CodegenTypeError::InvalidMapKey(key.clone()));
            }
            validate_map_keys_ty(value, pool, resolving_aliases)
        }
        Ty::Class(_, args, _) => args
            .iter()
            .try_for_each(|arg| validate_map_keys_ty(arg, pool, resolving_aliases)),
        Ty::Interface(_, generics, associated_types, _) => {
            generics
                .iter()
                .try_for_each(|ty| validate_map_keys_ty(ty, pool, resolving_aliases))?;
            associated_types
                .iter()
                .try_for_each(|(_, ty)| validate_map_keys_ty(ty, pool, resolving_aliases))
        }
        Ty::List(inner, _) => validate_map_keys_ty(inner, pool, resolving_aliases),
        Ty::Union(members, _) => members
            .iter()
            .try_for_each(|member| validate_map_keys_ty(member, pool, resolving_aliases)),
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for param in params {
                validate_map_keys_ty(&param.ty, pool, resolving_aliases)?;
            }
            validate_map_keys_ty(ret, pool, resolving_aliases)?;
            validate_map_keys_ty(throws, pool, resolving_aliases)
        }
        Ty::Future(value, error, _) => {
            validate_map_keys_ty(value, pool, resolving_aliases)?;
            validate_map_keys_ty(error, pool, resolving_aliases)
        }
        _ => Ok(()),
    }
}

fn map_key_resolves_to_string(
    key: &Ty,
    pool: &SymbolPool,
    resolving_aliases: &mut HashSet<super::Name>,
) -> bool {
    match key {
        Ty::String { .. } | Ty::Literal(baml_base::Literal::String(_), ..) => true,
        Ty::Never { .. } => true,
        Ty::Union(members, _) => members
            .iter()
            .all(|member| map_key_resolves_to_string(member, pool, resolving_aliases)),
        Ty::TypeAlias(name, _) => {
            if !resolving_aliases.insert(name.clone()) {
                return false;
            }
            let valid = matches!(pool.get(name), Some(Symbol::TypeAlias(alias)) if
                map_key_resolves_to_string(&alias.resolves_to, pool, resolving_aliases));
            resolving_aliases.remove(name);
            valid
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use baml_base::{Name as BaseName, TyAttr};

    use super::*;

    fn name(value: &str) -> crate::Name {
        crate::Name::new(BaseName::new("user"), Vec::new(), BaseName::new(value))
    }

    fn origin() -> Origin {
        Origin {
            source_file_path: "types.baml".to_string(),
            span_start: 0,
        }
    }

    fn alias(name: crate::Name, resolves_to: Ty) -> Symbol {
        Symbol::TypeAlias(TypeAlias {
            name,
            resolves_to,
            recursive: false,
            origin: origin(),
        })
    }

    fn map_with_key(key: Ty) -> Symbol {
        Symbol::Class(Class {
            name: name("Holder"),
            generic_params: Vec::new(),
            docstring: None,
            properties: vec![ClassProperty {
                name: BaseName::new("values"),
                docstring: None,
                ty: Ty::Map {
                    key: Box::new(key),
                    value: Box::new(Ty::Int {
                        attr: TyAttr::EMPTY,
                    }),
                    attr: TyAttr::EMPTY,
                },
            }],
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
            origin: origin(),
        })
    }

    #[test]
    fn map_key_validation_follows_alias_chains() {
        let key = name("Key");
        let key_chain = name("KeyChain");
        let mut pool = SymbolPool::new();
        pool.insert(
            key.clone(),
            alias(
                key.clone(),
                Ty::String {
                    attr: TyAttr::EMPTY,
                },
            ),
        );
        pool.insert(
            key_chain.clone(),
            alias(key_chain.clone(), Ty::TypeAlias(key, TyAttr::EMPTY)),
        );
        pool.insert(
            name("Holder"),
            map_with_key(Ty::TypeAlias(key_chain, TyAttr::EMPTY)),
        );

        assert_eq!(validate_symbol_pool_map_keys(&pool), Ok(()));
    }

    #[test]
    fn map_key_validation_rejects_non_string_and_cyclic_aliases() {
        for (label, target) in [
            (
                "non-string",
                Ty::Int {
                    attr: TyAttr::EMPTY,
                },
            ),
            ("cycle", Ty::TypeAlias(name("Key"), TyAttr::EMPTY)),
        ] {
            let key = name("Key");
            let mut pool = SymbolPool::new();
            pool.insert(key.clone(), alias(key.clone(), target));
            pool.insert(
                name("Holder"),
                map_with_key(Ty::TypeAlias(key, TyAttr::EMPTY)),
            );

            assert!(
                matches!(
                    validate_symbol_pool_map_keys(&pool),
                    Err(CodegenTypeError::InvalidMapKey(key))
                        if matches!(key.as_ref(), Ty::TypeAlias(..))
                ),
                "{label} alias key must be rejected"
            );
        }
    }

    #[test]
    fn union_walker_is_total_for_noncanonical_pools() {
        let nested = Ty::Union(
            Box::new([
                Ty::String {
                    attr: TyAttr::EMPTY,
                },
                Ty::Null {
                    attr: TyAttr::EMPTY,
                },
            ]),
            TyAttr::EMPTY,
        );
        let outer = Ty::Union(
            Box::new([
                Ty::Int {
                    attr: TyAttr::EMPTY,
                },
                nested.clone(),
            ]),
            TyAttr::EMPTY,
        );
        let symbol = alias(name("Nested"), outer.clone());

        assert_eq!(symbol.walk_all_unions(), HashSet::from([outer, nested]));
    }
}
