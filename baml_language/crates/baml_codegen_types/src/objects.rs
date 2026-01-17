use std::collections::HashSet;

use crate::{CodegenTypeError, Ty};

pub type ObjectPool = std::collections::HashMap<super::Name, Object>;

pub enum Object {
    Function(Function),
    Class(Class),
    Enum(Enum),
    TypeAlias(TypeAlias),
}

pub struct Function {
    pub name: baml_base::Name,
    pub docstring: Option<String>,
    pub arguments: Vec<FunctionArgument>,
    pub return_type: super::Ty,
    /// Only available if function streams
    /// Expresion functions don't stream for example
    pub stream_return_type: Option<super::Ty>,

    // TODO: add other APIs here that impact code-gen
    pub watchers: Vec<(baml_base::Name, super::Ty)>,
}

pub struct FunctionArgument {
    pub name: baml_base::Name,
    pub docstring: Option<String>,
    pub ty: super::Ty,
}

pub struct Class {
    pub name: super::Name,
    pub docstring: Option<String>,
    pub properties: Vec<ClassProperty>,
}

pub struct ClassProperty {
    pub name: baml_base::Name,
    pub docstring: Option<String>,
    pub ty: super::Ty,
}

pub struct Enum {
    pub name: super::Name,
    pub docstring: Option<String>,
    pub variants: Vec<EnumVariant>,
}

pub struct EnumVariant {
    pub name: baml_base::Name,
    pub docstring: Option<String>,
    pub value: String,
}

pub struct TypeAlias {
    pub name: super::Name,
    pub resolves_to: super::Ty,
}

impl Object {
    pub fn validate(&self) -> Result<(), CodegenTypeError> {
        match self {
            Object::Function(function) => function.validate(),
            Object::Class(class) => class.validate(),
            Object::Enum(_) => Ok(()),
            Object::TypeAlias(type_alias) => type_alias.validate(),
        }
    }

    pub fn walk_all_unions(&self) -> HashSet<super::Ty> {
        match self {
            Object::Function(function) => function.walk_all_unions(),
            Object::Class(class) => class.walk_all_unions(),
            Object::Enum(_) => HashSet::default(),
            Object::TypeAlias(type_alias) => type_alias.walk_all_unions(),
        }
    }
}

impl Function {
    fn validate(&self) -> Result<(), CodegenTypeError> {
        self.arguments
            .iter()
            .map(|args| args.ty.validate())
            .collect::<Result<Vec<_>, _>>()?;
        self.return_type.validate()?;
        if let Some(stream_type) = &self.stream_return_type {
            stream_type.validate()?;
        }

        Ok(())
    }

    fn walk_all_unions(&self) -> HashSet<super::Ty> {
        self.arguments
            .iter()
            .flat_map(|args| args.ty.walk_all_unions().into_iter())
            .chain(self.return_type.walk_all_unions())
            .chain(
                self.stream_return_type
                    .iter()
                    .flat_map(|ty| ty.walk_all_unions().into_iter()),
            )
            .collect()
    }
}

impl Class {
    fn validate(&self) -> Result<(), CodegenTypeError> {
        self.properties
            .iter()
            .map(|prop| prop.ty.validate())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(())
    }

    fn walk_all_unions(&self) -> HashSet<super::Ty> {
        self.properties
            .iter()
            .flat_map(|prop| prop.ty.walk_all_unions().into_iter())
            .collect::<_>()
    }
}

impl TypeAlias {
    fn validate(&self) -> Result<(), CodegenTypeError> {
        self.resolves_to.validate()
    }

    fn walk_all_unions(&self) -> HashSet<super::Ty> {
        self.resolves_to.walk_all_unions()
    }
}

impl super::Ty {
    pub fn walk_all_unions(&self) -> HashSet<super::Ty> {
        let mut unions = HashSet::<Ty>::default();
        if matches!(self, Ty::Union(_)) {
            unions.insert(self.clone());
        }

        match self {
            Ty::Int |
            Ty::Float  |
            Ty::String |
            Ty::Bool |
            Ty::Null |
            Ty::Unit |
            Ty::Media(_) |
            Ty::Class(_) |
            Ty::Enum(_) |
            // Unions are guranteed to not have unions thanks to .validate()
            Ty::Union(_) |
            Ty::Literal(_) => {},
            Ty::Optional(ty) |
            Ty::List(ty) |
            Ty::Checked(ty, _) |
            Ty::StreamState(ty) |
            Ty::Map { key: _, value: ty } => {
                unions.extend(ty.walk_all_unions());
            },
            Ty::BamlOptions => {},
        }

        unions
    }
}
