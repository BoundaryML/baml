//! A recorded type as BAML code that rebuilds it: what `baml_code(v)`
//! returns, so a test can use the exact type a recorded call had.
//!
//! The code is one block expression whose value is the type, as a
//! `reflect.Type`. Every class and enum built at run time that the type
//! names is rebuilt from its recorded definition with `reflect`, after the
//! ones it names, and bound with `let type` so that later field types can
//! name it. Classes that name each other are built with builders. A class
//! or enum declared in source is named as declared (`user.Resume` as
//! `Resume`): the code assumes the program it runs in declares it as the
//! recording did.
//!
//! What `reflect` cannot set is left as a comment: a runtime class's own
//! metadata, `@skip` and the streaming flags, and a field type that names a
//! class of its own group other than through lists, optionals and unions.

use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
};

use baml_type::{DeclarationName, HeadDisplay, TyTemplate};
use btel_snapshot::{
    CasId, DecodedDefinition, DecodedRoot, DefinitionHead, DefinitionRef, OwnedType, TypeIdentity,
    definition::{Declaration, DefinitionType, Meta},
};

use super::span::Span;

/// A head in a type the code spells: a name as written in source, or a
/// recorded definition the code names by its binding.
#[derive(Clone, Debug)]
enum Ref {
    Name(String),
    Definition(DefinitionRef),
}

/// A head as the code spells it.
#[derive(Clone, Debug)]
struct Spelled(String);

impl HeadDisplay for Spelled {
    fn head_display_name(&self) -> String {
        self.0.clone()
    }
}

/// The root of what `baml_code` renders: a type, or the type of an
/// instance or enum value.
pub(super) enum Start<'a> {
    Type(&'a OwnedType),
    /// A class or enum declaration object, and an instance's type arguments.
    Declaration {
        name: &'a DeclarationName,
        definition: Option<DefinitionRef>,
        is_enum: bool,
        arguments: Vec<&'a OwnedType>,
    },
}

/// The code rebuilding `start`, or `None` when a definition it names could
/// not be read.
pub(super) fn render(span: &Span, start: &Start<'_>) -> Option<String> {
    let root = match start {
        Start::Type(ty) => from_owned(ty),
        Start::Declaration {
            name,
            definition,
            is_enum,
            arguments,
        } => {
            let head = definition.map_or_else(|| Ref::Name(source_name(name)), Ref::Definition);
            if *is_enum {
                TyTemplate::Enum(head)
            } else {
                TyTemplate::Class(head, arguments.iter().map(|ty| from_owned(ty)).collect())
            }
        }
    };
    let mut code = Code {
        span,
        order: Vec::new(),
        seen: HashSet::new(),
        names: HashMap::new(),
        taken: HashSet::new(),
    };
    let mut heads = Vec::new();
    root.visit_heads(&mut |head| heads.push(head.clone()));
    for head in heads {
        if let Ref::Definition(definition) = head {
            code.visit(definition.group)?;
        }
    }
    let mut out = String::from("{\n");
    for group in code.order.clone() {
        code.group(group, &mut out)?;
    }
    writeln!(out, "    reflect.Type.of<{}>()", code.spell(&root)?).ok()?;
    out.push('}');
    Some(out)
}

struct Code<'s> {
    span: &'s Span,
    /// Groups of runtime definitions, each after the groups it names.
    order: Vec<CasId>,
    seen: HashSet<CasId>,
    /// How the code names each definition: its binding for a runtime one,
    /// its source name for a declared one.
    names: HashMap<DefinitionRef, String>,
    taken: HashSet<String>,
}

impl Code<'_> {
    fn members(&self, group: CasId) -> Option<&[DecodedDefinition]> {
        match &self.span.group(group).ok()?.root {
            DecodedRoot::Definitions(members) => Some(members),
            _ => None,
        }
    }

    /// Names every member of `group`, and orders its runtime groups after
    /// the groups they name.
    fn visit(&mut self, group: CasId) -> Option<()> {
        if !self.seen.insert(group) {
            return Some(());
        }
        let members = self.members(group)?.to_vec();
        let runtime = members
            .first()
            .is_some_and(|member| matches!(member.name(), DeclarationName::Anonymous(_)));
        for (position, member) in members.iter().enumerate() {
            let definition = DefinitionRef {
                group,
                member: u32::try_from(position).ok()?,
            };
            let name = if runtime {
                self.binding(&member.name().to_string())
            } else {
                source_name(member.name())
            };
            self.names.insert(definition, name);
        }
        // A declared class names only declared types, which need no code.
        if !runtime {
            return Some(());
        }
        for member in &members {
            let Declaration::Class(class) = member else {
                continue;
            };
            for field in &class.fields {
                let mut named = Vec::new();
                field.ty.decoded.as_ref()?.visit_heads(&mut |head| {
                    if let DefinitionHead::Defined(head) = head {
                        named.push(head.definition.group);
                    }
                });
                for other in named {
                    self.visit(other)?;
                }
            }
        }
        self.order.push(group);
        Some(())
    }

    /// A binding for a runtime class or enum called `name`: the name itself
    /// when it is a free identifier.
    fn binding(&mut self, name: &str) -> String {
        let mut base: String = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        if !base.starts_with(|c: char| c.is_ascii_alphabetic()) {
            base.insert(0, 'T');
        }
        if RESERVED.contains(&base.as_str()) {
            base.push('_');
        }
        let mut candidate = base.clone();
        let mut n = 2;
        while !self.taken.insert(candidate.clone()) {
            candidate = format!("{base}_{n}");
            n += 1;
        }
        candidate
    }

    /// The statements that build the runtime group `group`.
    fn group(&mut self, group: CasId, out: &mut String) -> Option<()> {
        let members = self.members(group)?.to_vec();
        let recursive = members.iter().any(|member| match member {
            Declaration::Class(class) => class.fields.iter().any(|field| {
                let mut found = false;
                if let Some(ty) = &field.ty.decoded {
                    ty.visit_heads(&mut |head| found |= matches!(head, DefinitionHead::Member(_)));
                }
                found
            }),
            Declaration::Enum(_) => false,
        });
        let name = |position: usize| -> Option<String> {
            let member = u32::try_from(position).ok()?;
            self.names.get(&DefinitionRef { group, member }).cloned()
        };
        if !recursive {
            for (position, member) in members.iter().enumerate() {
                let binding = name(position)?;
                match member {
                    Declaration::Class(class) => {
                        unsettable_class(class, out);
                        writeln!(
                            out,
                            "    let type {binding} = reflect.Class.new({}, {{",
                            quoted(&class.name.to_string())
                        )
                        .ok()?;
                        for field in &class.fields {
                            let ty = self.field_type(group, field.ty.decoded.as_deref()?)?;
                            write!(
                                out,
                                "        {}: {ty}{},",
                                quoted(&field.name),
                                meta(&field.meta)
                            )
                            .ok()?;
                            unsettable_field(field.skip, field.stream_done, field.must_exist, out);
                            out.push('\n');
                        }
                        out.push_str("    });\n");
                    }
                    Declaration::Enum(enm) => {
                        if enm.meta != Meta::default() {
                            out.push_str(
                                "    // recorded with enum metadata, which reflect can't set\n",
                            );
                        }
                        writeln!(
                            out,
                            "    let type {binding} = reflect.Enum.new({}, [",
                            quoted(&enm.name.to_string())
                        )
                        .ok()?;
                        for variant in &enm.variants {
                            if variant.meta == Meta::default() {
                                write!(out, "        {},", quoted(&variant.name)).ok()?;
                            } else {
                                write!(
                                    out,
                                    "        reflect.Enum.value({}{}),",
                                    quoted(&variant.name),
                                    meta_arguments(&variant.meta)
                                        .map(|arguments| format!(", {arguments}"))
                                        .unwrap_or_default()
                                )
                                .ok()?;
                            }
                            if variant.skip {
                                out.push_str(" // recorded with @skip, which reflect can't set");
                            }
                            out.push('\n');
                        }
                        out.push_str("    ]);\n");
                    }
                }
            }
            return Some(());
        }
        // Builders first, so that any field can name any member.
        for (position, member) in members.iter().enumerate() {
            let Declaration::Class(class) = member else {
                return None;
            };
            let binding = name(position)?;
            unsettable_class(class, out);
            writeln!(
                out,
                "    let {binding}_builder = reflect.Class.builder({});",
                quoted(&class.name.to_string())
            )
            .ok()?;
        }
        for (position, member) in members.iter().enumerate() {
            let Declaration::Class(class) = member else {
                return None;
            };
            let binding = name(position)?;
            for field in &class.fields {
                let ty = field.ty.decoded.as_deref()?;
                let Some(ty) = self.pending(group, ty).or_else(|| {
                    (!names_member(ty))
                        .then(|| self.field_type(group, ty))
                        .flatten()
                }) else {
                    writeln!(
                        out,
                        "    // field {} names its own group in a way reflect can't build",
                        quoted(&field.name)
                    )
                    .ok()?;
                    continue;
                };
                write!(
                    out,
                    "    {binding}_builder.field({}, {ty}{});",
                    quoted(&field.name),
                    meta(&field.meta)
                )
                .ok()?;
                unsettable_field(field.skip, field.stream_done, field.must_exist, out);
                out.push('\n');
            }
        }
        for position in 0..members.len() {
            let binding = name(position)?;
            writeln!(out, "    let type {binding} = {binding}_builder.build();").ok()?;
        }
        Some(())
    }

    /// A field type that names no member of its group, as a type value.
    fn field_type(&self, group: CasId, ty: &DefinitionType) -> Option<String> {
        let spelled = self.spell(&from_field(ty, group))?;
        Some(format!("reflect.Type.of<{spelled}>()"))
    }

    /// A field type that names a member of its group, as a pending type:
    /// the member, or lists, optionals and unions of it.
    fn pending(&self, group: CasId, ty: &DefinitionType) -> Option<String> {
        match ty {
            TyTemplate::Class(DefinitionHead::Member(member), arguments)
                if arguments.is_empty() =>
            {
                let name = self.names.get(&DefinitionRef {
                    group,
                    member: *member,
                })?;
                Some(format!("{name}_builder.type()"))
            }
            TyTemplate::List(item) if names_member(item) => {
                Some(format!("{}.array()", self.pending(group, item)?))
            }
            TyTemplate::Union(parts) if names_member(ty) => {
                let (first, rest) = parts.iter().position(names_member).map(|at| {
                    let rest: Vec<_> = parts
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != at)
                        .map(|(_, part)| part)
                        .collect();
                    (&parts[at], rest)
                })?;
                let first = self.pending(group, first)?;
                if let [TyTemplate::Null] = rest.as_slice() {
                    return Some(format!("{first}.optional()"));
                }
                let rest = rest
                    .iter()
                    .map(|part| {
                        if names_member(part) {
                            self.pending(group, part)
                        } else {
                            self.field_type(group, part)
                        }
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(format!("{first}.union([{}])", rest.join(", ")))
            }
            _ => None,
        }
    }

    /// `ty` in BAML type syntax, each definition by the name the code gives
    /// it.
    fn spell(&self, ty: &TyTemplate<Ref>) -> Option<String> {
        let mut missing = false;
        let spelled = ty.map_heads(&mut |head| match head {
            Ref::Name(name) => Spelled(name.clone()),
            Ref::Definition(definition) => match self.names.get(definition) {
                Some(name) => Spelled(name.clone()),
                None => {
                    missing = true;
                    Spelled(String::new())
                }
            },
        });
        (!missing).then(|| spelled.to_string())
    }
}

/// Words a binding must not be.
const RESERVED: &[&str] = &[
    "bool",
    "break",
    "catch",
    "class",
    "client",
    "continue",
    "else",
    "enum",
    "false",
    "float",
    "for",
    "function",
    "if",
    "image",
    "in",
    "int",
    "interface",
    "let",
    "map",
    "match",
    "null",
    "reflect",
    "return",
    "string",
    "test",
    "throw",
    "true",
    "type",
    "unknown",
    "void",
    "while",
];

fn names_member(ty: &DefinitionType) -> bool {
    let mut found = false;
    ty.visit_heads(&mut |head| found |= matches!(head, DefinitionHead::Member(_)));
    found
}

fn from_owned(ty: &OwnedType) -> TyTemplate<Ref> {
    TyTemplate::from(ty.map_heads(&mut |head| match head {
        TypeIdentity::Resolved(head) => Ref::Name(source_name(head.name())),
        TypeIdentity::Unresolved(_) => Ref::Name("unknown".into()),
        TypeIdentity::Defined(definition) => Ref::Definition(*definition),
    }))
}

fn from_field(ty: &DefinitionType, group: CasId) -> TyTemplate<Ref> {
    ty.map_heads(&mut |head| match head {
        DefinitionHead::Member(member) => Ref::Definition(DefinitionRef {
            group,
            member: *member,
        }),
        DefinitionHead::Defined(head) => Ref::Definition(head.definition),
        DefinitionHead::Named(name) => Ref::Name(source_name(name)),
    })
}

/// How source names a declared class or enum: its full name, without the
/// project's own package.
fn source_name(name: &DeclarationName) -> String {
    let name = name.to_string();
    name.strip_prefix("user.")
        .map_or(name.clone(), str::to_owned)
}

/// `text` as a BAML string literal.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `meta`'s arguments, `alias = …, description = …`, if it has any.
fn meta_arguments(meta: &Meta) -> Option<String> {
    let mut arguments = Vec::new();
    for (key, value) in [
        ("alias", &meta.alias),
        ("description", &meta.description),
        ("docstring", &meta.docstring),
    ] {
        if let Some(value) = value {
            arguments.push(format!("{key} = {}", quoted(value)));
        }
    }
    if !meta.attributes.is_empty() {
        let entries: Vec<String> = meta
            .attributes
            .iter()
            .map(|(key, value)| format!("{}: {}", quoted(key), quoted(value)))
            .collect();
        arguments.push(format!("other = {{{}}}", entries.join(", ")));
    }
    (!arguments.is_empty()).then(|| arguments.join(", "))
}

/// `.meta(…)` for a field type with metadata.
fn meta(meta: &Meta) -> String {
    meta_arguments(meta).map_or_else(String::new, |arguments| format!(".meta({arguments})"))
}

fn unsettable_class<T>(class: &btel_snapshot::definition::Class<T>, out: &mut String) {
    if class.meta != Meta::default() || class.stream_done {
        out.push_str("    // recorded with class metadata, which reflect can't set\n");
    }
}

fn unsettable_field(skip: bool, stream_done: bool, must_exist: bool, out: &mut String) {
    let flags: Vec<&str> = [
        (skip, "@skip"),
        (stream_done, "@stream.done"),
        (must_exist, "@stream.must_exist"),
    ]
    .into_iter()
    .filter_map(|(set, flag)| set.then_some(flag))
    .collect();
    if !flags.is_empty() {
        let _ = write!(
            out,
            " // recorded with {}, which reflect can't set",
            flags.join(" and ")
        );
    }
}
