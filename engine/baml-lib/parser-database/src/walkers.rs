//! Convenient access to a datamodel as understood by ParserDatabase.
//!
//! The walkers:
//! - Know about specific types and what kind they are (models, enums, etc.)
//! - Know about attributes and which ones are defined and allowed in a Prisma schema.
//! - Know about relations.
//! - Do not know anything about connectors, they are generic.

mod r#class;
mod client;
mod configuration;
mod r#enum;
mod field;
mod function;
mod variants;

pub use client::*;
pub use configuration::*;
use either::Either;
pub use field::*;
pub use function::*;
use internal_baml_diagnostics::Span;
use internal_baml_schema_ast::ast::{Identifier, TopId, WithName};
pub use r#class::*;
pub use r#enum::*;
pub use variants::*;

/// A generic walker. Only walkers intantiated with a concrete ID type (`I`) are useful.
#[derive(Clone, Copy)]
pub struct Walker<'db, I> {
    /// The parser database being traversed.
    pub db: &'db crate::ParserDatabase,
    /// The identifier of the focused element.
    pub id: I,
}

impl<'db, I> Walker<'db, I> {
    /// Traverse something else in the same schema.
    pub fn walk<J>(self, other: J) -> Walker<'db, J> {
        self.db.walk(other)
    }
}

impl<'db, I> PartialEq for Walker<'db, I>
where
    I: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl crate::ParserDatabase {
    /// Find an enum by name.
    pub fn find_enum<'db>(&'db self, idn: &Identifier) -> Option<EnumWalker<'db>> {
        self.find_type(idn).and_then(|either| match either {
            Either::Right(class) => Some(class),
            _ => None,
        })
    }

    /// Find a type by name.
    pub fn find_type_by_str<'db>(
        &'db self,
        name: &str,
    ) -> Option<Either<ClassWalker<'db>, EnumWalker<'db>>> {
        self.interner
            .lookup(name)
            .and_then(|name_id| self.names.tops.get(&name_id))
            .and_then(|top_id| match top_id {
                TopId::Class(class_id) => Some(Either::Left(self.walk(*class_id))),
                TopId::Enum(enum_id) => Some(Either::Right(self.walk(*enum_id))),
                _ => None,
            })
    }

    /// Find the correct printer.
    pub fn find_printer<'db>(&'db self, printer: &Option<(String, Span)>) -> Option<&str> {
        match printer {
            Some((..)) => None,
            None => None,
        }
    }

    /// Find a type by name.
    pub fn find_type<'db>(
        &'db self,
        idn: &Identifier,
    ) -> Option<Either<ClassWalker<'db>, EnumWalker<'db>>> {
        match idn {
            Identifier::Local(local, _) => self.find_type_by_str(local),
            _ => None,
        }
    }

    /// Find a model by name.
    pub fn find_class<'db>(&'db self, idn: &Identifier) -> Option<ClassWalker<'db>> {
        self.find_type(idn).and_then(|either| match either {
            Either::Left(class) => Some(class),
            _ => None,
        })
    }

    /// Find a client by name.
    pub fn find_client<'db>(&'db self, name: &str) -> Option<ClientWalker<'db>> {
        self.interner
            .lookup(name)
            .and_then(|name_id| self.names.tops.get(&name_id))
            .and_then(|top_id| top_id.as_client_id())
            .map(|model_id| self.walk(model_id))
    }

    /// Find a function by name.
    pub fn find_function<'db>(&'db self, idn: &Identifier) -> Option<FunctionWalker<'db>> {
        self.interner
            .lookup(idn.name())
            .and_then(|name_id| self.names.tops.get(&name_id))
            .and_then(|top_id| top_id.as_function_id())
            .map(|model_id| self.walk(model_id))
    }

    /// Find a function by name.
    pub fn find_retry_policy<'db>(&'db self, name: &str) -> Option<RetryPolicyWalker<'db>> {
        self.interner
            .lookup(name)
            .and_then(|name_id| self.names.tops.get(&name_id))
            .and_then(|top_id| top_id.as_retry_policy_id())
            .map(|model_id| self.walk(model_id))
    }

    /// Traverse a schema element by id.
    pub fn walk<I>(&self, id: I) -> Walker<'_, I> {
        Walker { db: self, id }
    }

    /// Get all the types that are valid in the schema. (including primitives)
    pub fn valid_type_names<'db>(&'db self) -> Vec<String> {
        let mut names: Vec<String> = self.walk_classes().map(|c| c.name().to_string()).collect();
        names.extend(self.walk_enums().map(|e| e.name().to_string()));
        // Add primitive types
        names.extend(
            vec!["string", "int", "float", "bool"]
                .into_iter()
                .map(String::from),
        );
        names
    }

    /// Get all the types that are valid in the schema. (including primitives)
    pub fn valid_function_names(&self) -> Vec<String> {
        self.walk_functions()
            .map(|c| c.name().to_string())
            .collect()
    }

    /// Get all the types that are valid in the schema. (including primitives)
    pub fn valid_retry_policy_names(&self) -> Vec<String> {
        self.walk_retry_policies()
            .map(|c| c.name().to_string())
            .collect()
    }

    /// Get all the types that are valid in the schema. (including primitives)
    pub fn valid_client_names(&self) -> Vec<&str> {
        self.walk_clients().map(|c| c.name()).collect()
    }

    /// Walk all enums in the schema.
    pub fn walk_enums(&self) -> impl Iterator<Item = EnumWalker<'_>> {
        self.ast()
            .iter_tops()
            .filter_map(|(top_id, _)| top_id.as_enum_id())
            .map(move |top_id| Walker {
                db: self,
                id: top_id,
            })
    }

    /// Walk all classes in the schema.
    pub fn walk_classes(&self) -> impl Iterator<Item = ClassWalker<'_>> {
        self.ast()
            .iter_tops()
            .filter_map(|(top_id, _)| top_id.as_class_id())
            .map(move |top_id| Walker {
                db: self,
                id: top_id,
            })
    }

    /// Walk all classes in the schema.
    pub fn walk_functions(&self) -> impl Iterator<Item = FunctionWalker<'_>> {
        self.ast()
            .iter_tops()
            .filter_map(|(top_id, _)| top_id.as_function_id())
            .map(move |top_id| Walker {
                db: self,
                id: top_id,
            })
    }

    /// Walk all classes in the schema.
    pub fn walk_clients(&self) -> impl Iterator<Item = ClientWalker<'_>> {
        self.ast()
            .iter_tops()
            .filter_map(|(top_id, _)| top_id.as_client_id())
            .map(move |top_id| Walker {
                db: self,
                id: top_id,
            })
    }

    /// Walk all variants in the schema.
    pub fn walk_variants(&self) -> impl Iterator<Item = VariantWalker<'_>> {
        self.ast()
            .iter_tops()
            .filter_map(|(top_id, _)| top_id.as_variant_id())
            .map(move |top_id| Walker {
                db: self,
                id: top_id,
            })
    }

    /// Walk all classes in the schema.
    pub fn walk_retry_policies(&self) -> impl Iterator<Item = RetryPolicyWalker<'_>> {
        self.ast()
            .iter_tops()
            .filter_map(|(top_id, _)| top_id.as_retry_policy_id())
            .map(move |top_id| Walker {
                db: self,
                id: top_id,
            })
    }
}
