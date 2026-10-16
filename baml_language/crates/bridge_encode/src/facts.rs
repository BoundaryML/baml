//! The declared types a walk reads, compiled once from a program descriptor.
//!
//! Every type here is in *stated form*: the form a node states on the wire.
//! Type aliases are transparent, and only a recursive one exists as a
//! declaration at run time, so a non-recursive alias is spelled by its body
//! everywhere and a recursive one by its name. Nested unions are flattened
//! and duplicate members dropped once, here, so the walk never re-derives a
//! slot's shape per value.

use std::collections::{HashMap, HashSet};

use baml_sdkgen_types::ProgramDescriptor;
use baml_type::{Name, ParamTy, RuntimeTy};

use crate::{error::FactsError, host::Head};

/// Every signature and declaration of one program, in the runtime type family
/// and in stated form.
#[derive(Debug, Default)]
pub struct Facts {
    functions: HashMap<String, FunctionSig>,
    classes: HashMap<Head, ClassFacts>,
    enums: HashMap<Head, EnumFacts>,
    /// The recursive aliases: the names the engine resolves at run time, with
    /// their bodies.
    recursive: HashMap<Head, RuntimeTy>,
    /// The non-recursive aliases, by body, for a type that arrives from the
    /// host spelling one (an explicit type argument, an instance's own
    /// arguments).
    expanded: HashMap<Head, RuntimeTy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSig {
    /// Type parameters of the enclosing class, first in De Bruijn order.
    pub class_generic_params: Vec<Name>,
    /// The callable's own type parameters.
    pub generic_params: Vec<Name>,
    pub params: Vec<Param>,
    pub ret: RuntimeTy,
    pub throws: RuntimeTy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: RuntimeTy,
    pub has_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassFacts {
    pub name: Head,
    pub generic_params: Vec<Name>,
    /// Field types mention the class's parameters as `TypeVar`s indexed by
    /// position in `generic_params`.
    pub fields: Vec<(String, RuntimeTy)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumFacts {
    pub name: Head,
    pub variants: Vec<String>,
}

impl Facts {
    pub fn from_descriptor(descriptor: &ProgramDescriptor) -> Result<Self, FactsError> {
        let raw: HashMap<Head, (RuntimeTy, bool)> = descriptor
            .type_aliases
            .iter()
            .map(|alias| {
                (
                    alias.name.clone(),
                    (RuntimeTy::from(&alias.resolves_to), alias.recursive),
                )
            })
            .collect();
        let state = |ty: RuntimeTy| stated(ty, &raw, &mut Vec::new());
        let mut facts = Facts::default();
        for function in &descriptor.functions {
            let params = function
                .params
                .iter()
                .map(|param| {
                    Ok(Param {
                        name: param.name.as_str().to_string(),
                        ty: state(RuntimeTy::from(&param.ty))?,
                        has_default: param.has_default,
                    })
                })
                .collect::<Result<_, FactsError>>()?;
            facts.functions.insert(
                function.fqn.clone(),
                FunctionSig {
                    class_generic_params: function.class_generic_params.clone(),
                    generic_params: function.generic_params.clone(),
                    params,
                    ret: state(RuntimeTy::from(&function.ret))?,
                    throws: state(RuntimeTy::from(&function.throws))?,
                },
            );
        }
        for class in &descriptor.classes {
            let fields = class
                .fields
                .iter()
                .map(|field| {
                    Ok((
                        field.name.as_str().to_string(),
                        state(RuntimeTy::from(&field.ty))?,
                    ))
                })
                .collect::<Result<_, FactsError>>()?;
            facts.classes.insert(
                class.name.clone(),
                ClassFacts {
                    name: class.name.clone(),
                    generic_params: class.generic_params.clone(),
                    fields,
                },
            );
        }
        for enumeration in &descriptor.enums {
            facts.enums.insert(
                enumeration.name.clone(),
                EnumFacts {
                    name: enumeration.name.clone(),
                    variants: enumeration
                        .variants
                        .iter()
                        .map(|variant| variant.as_str().to_string())
                        .collect(),
                },
            );
        }
        for (name, (body, recursive)) in &raw {
            let body = state(body.clone())?;
            if *recursive {
                facts.recursive.insert(name.clone(), body);
            } else {
                facts.expanded.insert(name.clone(), body);
            }
        }
        Ok(facts)
    }

    /// `ty` in stated form, for a type that arrives from the host spelling
    /// an alias. An alias the facts do not know is left as it is, for the
    /// engine to refuse by name.
    pub fn stated_form(&self, ty: RuntimeTy) -> RuntimeTy {
        match ty {
            RuntimeTy::TypeAlias(name) if !self.recursive.contains_key(&name) => {
                match self.expanded.get(&name) {
                    Some(body) => body.clone(),
                    None => RuntimeTy::TypeAlias(name),
                }
            }
            RuntimeTy::Union(members) => {
                let mut out: Vec<RuntimeTy> = Vec::with_capacity(members.len());
                for member in &members {
                    match self.stated_form(member.clone()) {
                        RuntimeTy::Union(nested) => {
                            for member in &nested {
                                push_distinct(&mut out, member.clone());
                            }
                        }
                        member => push_distinct(&mut out, member),
                    }
                }
                union_of(out)
            }
            other => other.map_children(&mut |child| self.stated_form(child.clone())),
        }
    }

    pub fn function(&self, fqn: &str) -> Option<&FunctionSig> {
        self.functions.get(fqn)
    }

    /// The class `name` declares; `None` for a name the descriptor does not
    /// reach (a declaration the program made at run time), which the host
    /// names as it has it and the engine judges.
    pub fn class(&self, name: &Head) -> Option<&ClassFacts> {
        self.classes.get(name)
    }

    pub fn enumeration(&self, name: &Head) -> Option<&EnumFacts> {
        self.enums.get(name)
    }

    /// `ty` with a recursive alias at its head unfolded one layer: the body
    /// a value under the alias is judged against. Every other type is itself.
    pub(crate) fn unfold<'a>(&'a self, mut ty: &'a RuntimeTy) -> &'a RuntimeTy {
        let mut seen: Vec<&Head> = Vec::new();
        while let RuntimeTy::TypeAlias(name) = ty {
            // An unguarded cycle of aliases (`type A = B; type B = A`) denotes
            // `never`; the body is left at the first name seen twice, which
            // no value inhabits.
            if seen.contains(&name) {
                break;
            }
            seen.push(name);
            match self.recursive.get(name) {
                Some(body) => ty = body,
                None => break,
            }
        }
        ty
    }

    /// The members of a union, with every recursive alias member unfolded and
    /// nested unions flattened, in declared order. A recursive member reached
    /// again on the same path contributes nothing: a self-supporting cycle
    /// proves no membership (`type A = A | A[]` is the type of `A[]`).
    pub(crate) fn flatten<'a>(&'a self, members: &'a [RuntimeTy]) -> Vec<&'a RuntimeTy> {
        let mut out = Vec::with_capacity(members.len());
        let mut seen = HashSet::new();
        self.flatten_into(members, &mut seen, &mut out);
        out
    }

    fn flatten_into<'a>(
        &'a self,
        members: &'a [RuntimeTy],
        seen: &mut HashSet<&'a Head>,
        out: &mut Vec<&'a RuntimeTy>,
    ) {
        for member in members {
            match member {
                RuntimeTy::TypeAlias(name) => {
                    if !seen.insert(name) {
                        continue;
                    }
                    match self.recursive.get(name) {
                        Some(RuntimeTy::Union(nested)) => self.flatten_into(nested, seen, out),
                        Some(body) => {
                            if !out.contains(&body) {
                                out.push(body);
                            }
                        }
                        None => {
                            if !out.contains(&member) {
                                out.push(member);
                            }
                        }
                    }
                    seen.remove(name);
                }
                RuntimeTy::Union(nested) => self.flatten_into(nested, seen, out),
                other => {
                    if !out.contains(&other) {
                        out.push(other);
                    }
                }
            }
        }
    }
}

/// `ty` with every non-recursive alias spelled by its body and every union
/// flat and duplicate-free.
fn stated(
    ty: RuntimeTy,
    aliases: &HashMap<Head, (RuntimeTy, bool)>,
    path: &mut Vec<Head>,
) -> Result<RuntimeTy, FactsError> {
    match ty {
        RuntimeTy::TypeAlias(name) => match aliases.get(&name) {
            Some((_, true)) => Ok(RuntimeTy::TypeAlias(name)),
            Some((body, false)) => {
                if path.contains(&name) {
                    return Err(FactsError::AliasCycle(name));
                }
                path.push(name.clone());
                let body = stated(body.clone(), aliases, path)?;
                path.pop();
                Ok(body)
            }
            None => Err(FactsError::UnknownAlias(name)),
        },
        RuntimeTy::Union(members) => {
            let mut out: Vec<RuntimeTy> = Vec::with_capacity(members.len());
            for member in &members {
                match stated(member.clone(), aliases, path)? {
                    RuntimeTy::Union(nested) => {
                        for member in &nested {
                            push_distinct(&mut out, member.clone());
                        }
                    }
                    member => push_distinct(&mut out, member),
                }
            }
            Ok(union_of(out))
        }
        other => {
            let mut failure = None;
            let mapped =
                other.map_children(&mut |child| match stated(child.clone(), aliases, path) {
                    Ok(child) => child,
                    Err(error) => {
                        failure.get_or_insert(error);
                        child.clone()
                    }
                });
            match failure {
                Some(error) => Err(error),
                None => Ok(mapped),
            }
        }
    }
}

fn push_distinct(members: &mut Vec<RuntimeTy>, member: RuntimeTy) {
    if !members.contains(&member) {
        members.push(member);
    }
}

/// A union of `members`, or the one member alone.
fn union_of(mut members: Vec<RuntimeTy>) -> RuntimeTy {
    match members.len() {
        1 => members.pop().expect("one member"),
        _ => RuntimeTy::Union(members.into()),
    }
}

/// `ty` with every `TypeVar` that `resolve` answers replaced by its answer.
/// A variable `resolve` declines stays a `TypeVar`.
pub(crate) fn substitute(
    ty: &RuntimeTy,
    resolve: &impl Fn(&ParamTy) -> Option<RuntimeTy>,
) -> RuntimeTy {
    match ty {
        RuntimeTy::TypeVar(param) => resolve(param).unwrap_or_else(|| ty.clone()),
        other => other.map_children(&mut |child| substitute(child, resolve)),
    }
}

/// Does `ty` mention a type variable anywhere?
pub(crate) fn mentions_type_var(ty: &RuntimeTy) -> bool {
    if matches!(ty, RuntimeTy::TypeVar(_)) {
        return true;
    }
    let mut found = false;
    ty.visit_children(&mut |child| found = found || mentions_type_var(child));
    found
}

/// The first type variable `ty` mentions, if any.
pub(crate) fn first_type_var(ty: &RuntimeTy) -> Option<ParamTy> {
    if let RuntimeTy::TypeVar(param) = ty {
        return Some(param.clone());
    }
    let mut found = None;
    ty.visit_children(&mut |child| {
        if found.is_none() {
            found = first_type_var(child);
        }
    });
    found
}
