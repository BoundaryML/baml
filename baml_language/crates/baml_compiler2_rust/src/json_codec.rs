//! Per-type JSON decoders and encoders for `baml.json.decode<T>` and
//! `baml.json.stringify` sites.
//!
//! The shape is the old runtime branch's (`brr/runtime-v2` `rt/json.rs`): each
//! type a decode site names, and every type it reaches, becomes one static
//! schema in the generated module, and a decode site reads the text through
//! the schema (`bex_vm::compiled_rt::json::decode_json`). The schema holds
//! what the interpreter works out per call: each field's JSON key, its slot
//! offset and kind, and its type's own schema. The engine writes instances in
//! place, through the same layout compiled code reads.
//!
//! The rules are the interpreter's one-pass text decoder's, and so is the
//! contract: exact where it succeeds, silent where it fails. On any miss the
//! site runs the call it replaced, which decodes again and raises the error
//! `decode` has always raised. So a type gets a schema only when the rules
//! the schema leaves out cannot apply to it:
//!
//! - every field's key is its name under either naming (no `@alias`,
//!   `@json.rename`, `@skip`, `@json.flatten` or `@json.fallback`);
//! - no `baml.FromJson` rule can provide its decode, and for an encode no
//!   `baml.ToJson` rule its rendering;
//! - for an encode, no class can reach itself, so no value can contain
//!   itself (the interpreter refuses such a value);
//! - a class has no type parameters and no `cleanup`, and is a user class
//!   (media and the stdlib's own classes decode by their own rules);
//! - a union is `T | null`, `int | float`, string literals, or classes told
//!   apart by a
//!   required string-literal field under one key, so trying members in
//!   declaration order picks the member the interpreter's ranking picks.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use baml_type::Literal;
use bex_vm_types::{
    Object, ObjectIndex, TyTemplate,
    layout::{RawKind, SlotKind},
};

use crate::index::ProgramIndex;

/// Which way a site converts.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Decode,
    Encode,
}

/// The schemas one generated module declares.
#[derive(Default)]
pub(crate) struct JsonCodecs {
    /// The static naming each type's schema, once declared.
    names: HashMap<TyTemplate, String>,
    /// The classes whose schemas are declared or being declared.
    classes: HashMap<usize, String>,
    statics: String,
}

impl JsonCodecs {
    /// The static holding `ty`'s schema, declaring it and every schema it
    /// reaches, or `None` when `ty` must decode through the interpreter.
    pub(crate) fn schema_for(
        &mut self,
        index: &ProgramIndex<'_>,
        ty: &TyTemplate,
        direction: Direction,
    ) -> Option<String> {
        let rules = Rules::new(index, direction);
        if !rules.eligible(ty, &mut HashSet::new()) {
            return None;
        }
        Some(self.schema(&rules, ty))
    }

    /// The module's schema statics.
    pub(crate) fn statics(&self) -> &str {
        &self.statics
    }

    fn schema(&mut self, rules: &Rules<'_, '_>, ty: &TyTemplate) -> String {
        if let Some(name) = self.names.get(ty) {
            return name.clone();
        }
        let name = format!("JSON_{}", self.names.len());
        self.names.insert(ty.clone(), name.clone());
        let value = self.codec(rules, ty);
        let _ = writeln!(
            self.statics,
            "static {name}: bex_vm::compiled_rt::json::Codec = bex_vm::compiled_rt::json::Codec::{value};"
        );
        name
    }

    /// The `Codec` variant for an eligible `ty`.
    fn codec(&mut self, rules: &Rules<'_, '_>, ty: &TyTemplate) -> String {
        match ty {
            TyTemplate::Int => "Int".into(),
            TyTemplate::Float => "Float".into(),
            TyTemplate::Bool => "Bool".into(),
            TyTemplate::String => "String".into(),
            TyTemplate::Literal(Literal::String(text), _) => format!("Literal({:?})", text.as_str()),
            TyTemplate::List(element) => format!("List(&{})", self.schema(rules, element)),
            TyTemplate::Map { value, .. } => format!("Map(&{})", self.schema(rules, value)),
            TyTemplate::Class(head, _) => {
                let class = head.tag().static_index().expect("eligible");
                format!("Class(&{})", self.class(rules, class))
            }
            TyTemplate::Union(members) => self.union(rules, members),
            _ => unreachable!("only eligible types get a schema"),
        }
    }

    fn union(&mut self, rules: &Rules<'_, '_>, members: &[TyTemplate]) -> String {
        let others: Vec<&TyTemplate> = members.iter().filter(|m| !is_null(m)).collect();
        if others.len() < members.len() {
            let inner = match others.as_slice() {
                [one] => self.schema(rules, one),
                _ => self.schema(rules, &TyTemplate::Union(others.into_iter().cloned().collect())),
            };
            return format!("Optional(&{inner})");
        }
        if let Some(texts) = string_literals(members) {
            return format!("Literals(&{texts:?})");
        }
        if is_number(members) {
            return "IntOrFloat".into();
        }
        let classes: Vec<String> = members
            .iter()
            .map(|member| {
                let TyTemplate::Class(head, _) = member else {
                    unreachable!("an eligible union of classes")
                };
                format!("&{}", self.class(rules, head.tag().static_index().expect("eligible")))
            })
            .collect();
        format!("Classes(&[{}])", classes.join(", "))
    }

    /// The `ClassCodec` static of `class`.
    fn class(&mut self, rules: &Rules<'_, '_>, class: usize) -> String {
        if let Some(name) = self.classes.get(&class) {
            return name.clone();
        }
        let name = format!("JSON_CLASS_{class}");
        self.classes.insert(class, name.clone());
        let declaration = rules.class(class).expect("eligible");
        let mut fields = String::new();
        for (i, field) in declaration.fields.iter().enumerate() {
            let slot = rules.index.field_slot(class, i).expect("a class field");
            let schema = self.schema(rules, &field.field_template);
            let _ = write!(
                fields,
                "bex_vm::compiled_rt::json::FieldCodec {{ key: {:?}, codec: &{schema}, offset: {}, slot: bex_vm::compiled_rt::json::Slot::{}, absent_null: {}, omit_null: {} }}, ",
                field.name,
                slot.offset,
                slot_name(slot.kind),
                accepts_null(&field.field_template),
                field.json_omit_null,
            );
        }
        let _ = writeln!(
            self.statics,
            "static {name}: bex_vm::compiled_rt::json::ClassCodec = bex_vm::compiled_rt::json::ClassCodec {{ class: {class}, fields: &[{fields}] }};"
        );
        name
    }
}

/// What decides whether a type gets a schema.
struct Rules<'i, 'p> {
    index: &'i ProgramIndex<'p>,
    direction: Direction,
    /// Classes a `baml.FromJson` (decode) or `baml.ToJson` (encode) rule
    /// names; `None` when a rule is generic, so it may name any class.
    overridden: Option<HashSet<usize>>,
}

impl<'i, 'p> Rules<'i, 'p> {
    fn new(index: &'i ProgramIndex<'p>, direction: Direction) -> Self {
        let interface = match direction {
            Direction::Decode => "FromJson",
            Direction::Encode => "ToJson",
        };
        Self {
            index,
            direction,
            overridden: index.rule_classes(interface),
        }
    }

    /// Whether `ty` and everything it reaches convert by the schema's rules.
    /// `visiting` holds the classes on the path from the site's type: met
    /// again, a class is decided by its first visit for a decode, and is a
    /// cycle an encode refuses.
    fn eligible(&self, ty: &TyTemplate, visiting: &mut HashSet<usize>) -> bool {
        match ty {
            TyTemplate::Int | TyTemplate::Float | TyTemplate::Bool | TyTemplate::String => true,
            TyTemplate::Literal(Literal::String(_), _) => true,
            TyTemplate::List(element) => self.eligible(element, visiting),
            TyTemplate::Map { key, value } => {
                matches!(**key, TyTemplate::String) && self.eligible(value, visiting)
            }
            TyTemplate::Class(head, args) if args.is_empty() => head
                .tag()
                .static_index()
                .is_some_and(|class| self.class_eligible(class, visiting)),
            TyTemplate::Union(members) => self.union_eligible(members, visiting),
            _ => false,
        }
    }

    fn union_eligible(&self, members: &[TyTemplate], visiting: &mut HashSet<usize>) -> bool {
        let others: Vec<TyTemplate> = members.iter().filter(|m| !is_null(m)).cloned().collect();
        match others.as_slice() {
            [] => false,
            [one] if others.len() < members.len() => self.eligible(one, visiting),
            _ if string_literals(&others).is_some() || is_number(&others) => true,
            _ => {
                others.iter().all(|member| self.eligible(member, visiting))
                    && self.told_apart(&others)
            }
        }
    }

    /// Whether every member is a class with a required string-literal field
    /// under one shared key, each with its own literal.
    fn told_apart(&self, members: &[TyTemplate]) -> bool {
        let Some(classes) = members
            .iter()
            .map(|member| match member {
                TyTemplate::Class(head, _) => self.class(head.tag().static_index()?),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let tags = |class: &bex_vm_types::types::Class| -> Vec<(String, String)> {
            class
                .fields
                .iter()
                .filter_map(|field| match &field.field_template {
                    TyTemplate::Literal(Literal::String(text), _) => {
                        Some((field.name.clone(), text.as_str().to_owned()))
                    }
                    _ => None,
                })
                .collect()
        };
        let first = tags(classes[0]);
        first.iter().any(|(key, _)| {
            let mut seen = HashSet::new();
            classes.iter().all(|class| {
                tags(class)
                    .into_iter()
                    .find(|(name, _)| name == key)
                    .is_some_and(|(_, text)| seen.insert(text))
            })
        })
    }

    fn class_eligible(&self, class: usize, visiting: &mut HashSet<usize>) -> bool {
        if !visiting.insert(class) {
            return self.direction == Direction::Decode;
        }
        let eligible = self.class_fields_eligible(class, visiting);
        if self.direction == Direction::Encode {
            visiting.remove(&class);
        }
        eligible
    }

    fn class_fields_eligible(&self, class: usize, visiting: &mut HashSet<usize>) -> bool {
        let Some(declaration) = self.class(class) else {
            return false;
        };
        let overridden = self
            .overridden
            .as_ref()
            .is_none_or(|classes| classes.contains(&class));
        !overridden
            && self.index.is_user_class(class)
            && declaration.generic_param_count == 0
            && declaration.cleanup.is_none()
            && declaration
                .fields
                .iter()
                .all(|field| keyed_by_name(field) && self.eligible(&field.field_template, visiting))
    }

    fn class(&self, class: usize) -> Option<&'p bex_vm_types::types::Class> {
        match &self.index.program().objects[ObjectIndex::from_raw(class)] {
            Object::Class(declaration) => Some(declaration),
            _ => None,
        }
    }
}

/// A field whose JSON key is its name under either naming, and which is
/// written and read as itself.
fn keyed_by_name(field: &bex_vm_types::ClassField) -> bool {
    field.alias.is_none()
        && field.json_rename.is_none()
        && !field.skip
        && !field.json_flatten
        && !field.json_fallback
}

fn is_null(ty: &TyTemplate) -> bool {
    matches!(ty, TyTemplate::Null)
}

/// A field whose type takes `null` reads as `null` when its key is absent.
fn accepts_null(ty: &TyTemplate) -> bool {
    match ty {
        TyTemplate::Null => true,
        TyTemplate::Union(members) => members.iter().any(accepts_null),
        _ => false,
    }
}

/// Whether a union is exactly `int | float`.
fn is_number(members: &[TyTemplate]) -> bool {
    members.len() == 2
        && members.contains(&TyTemplate::Int)
        && members.contains(&TyTemplate::Float)
}

/// The texts of a union of string literals only.
fn string_literals(members: &[TyTemplate]) -> Option<Vec<String>> {
    members
        .iter()
        .map(|member| match member {
            TyTemplate::Literal(Literal::String(text), _) => Some(text.as_str().to_owned()),
            _ => None,
        })
        .collect()
}

fn slot_name(kind: SlotKind) -> &'static str {
    match kind {
        SlotKind::Raw(RawKind::Int) => "Int",
        SlotKind::Raw(RawKind::Float) => "Float",
        SlotKind::Raw(RawKind::Bool) => "Bool",
        SlotKind::Ptr => "Ptr",
        SlotKind::Tagged16 => "Tagged",
    }
}
