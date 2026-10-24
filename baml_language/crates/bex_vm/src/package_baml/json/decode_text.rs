//! Decoding JSON text straight into a declared type, in one pass.
//!
//! The fast path of `decode<T>` when no `baml.FromJson` override can be
//! reached from `T`, and of the structural decode `OpenAI` structured outputs
//! use. The text is read once: keys the type does not declare are checked and
//! skipped without building anything, and the result is built as the text is
//! read.
//!
//! It is exact where it succeeds and silent where it fails. On any failure the
//! caller decodes again by the slow path, which reports the error that entry
//! point has always reported: a `ParseError` before any `DecodeError`, the
//! first failing field in declaration order, a bad map key before a bad map
//! value, a repeated key read by its last value. None of that ordering has to
//! be reproduced here.

use std::sync::Arc;

use baml_type::MediaKind;
use bex_heap::TlabHolder;
use bex_vm_types::{
    BexStr, Object, RealizedTy, TypeHead, Value,
    types::{Array, Instance},
};

use super::{
    MediaFields,
    plan::Plans,
    text::{Entries, Parser, Reader, Stop},
    union::{Candidates, Keys, Kind},
};
use crate::BexVm;

/// `text` decoded as `ty`, or `None` when the slow path must decide.
pub(super) fn decode(
    vm: &mut BexVm,
    text: &str,
    ty: &RealizedTy,
    plans: &mut Plans,
) -> Option<Value> {
    let mut decoder = TextDecoder {
        parser: Parser {
            reader: Reader::new(text),
            json: super::json_alias_ty(vm),
        },
        plans,
    };
    let value = decoder.value(vm, ty).ok()?;
    decoder.parser.reader.finish().ok()?;
    Some(value)
}

struct TextDecoder<'a, 'p> {
    /// Builds the `json` and `unknown` parts of the result.
    parser: Parser<'a>,
    plans: &'p mut Plans,
}

impl TextDecoder<'_, '_> {
    /// The value at the cursor, as `ty`.
    fn value(&mut self, vm: &mut BexVm, ty: &RealizedTy) -> Result<Value, Stop> {
        let reader = &mut self.parser.reader;
        match ty {
            RealizedTy::Null => reader.null().map(|()| Value::NULL),
            RealizedTy::Bool => reader.bool().map(Value::bool),
            RealizedTy::Int => reader
                .number()?
                .as_i64()
                .and_then(Value::try_int)
                .ok_or(Stop),
            RealizedTy::Bigint => {
                let bigint = self.bigint()?;
                vm.try_alloc_bigint(Arc::new(bigint)).map_err(|_| Stop)
            }
            RealizedTy::Float => {
                // A fraction or exponent past `f64` is a syntax error; an
                // integer that large is a bigint, which no `float` holds.
                let float = reader.number()?.as_f64().ok_or(Stop)?;
                Ok(Value::object(vm.alloc_float(float)))
            }
            RealizedTy::String => {
                let text = reader.string()?;
                Ok(Value::object(vm.alloc_string(text)))
            }
            RealizedTy::List(element) => self.list(vm, element),
            RealizedTy::Map { key, value } => self.map(vm, key, value),
            RealizedTy::TypeAlias(head) if vm.stdlib_heads().is_json_alias(*head) => {
                self.parser.value(vm)
            }
            RealizedTy::TypeAlias(_) => self.alias(vm, ty),
            RealizedTy::Class(head, type_args) => match vm.stdlib_heads().media_kind(*head) {
                Some(kind) => self.media(vm, kind, *head),
                None => self.class(vm, *head, type_args),
            },
            RealizedTy::Enum(head) => {
                let variant = reader.string()?;
                let index = super::variant_named(vm, *head, variant, self.plans.naming)
                    .ok()
                    .flatten()
                    .ok_or(Stop)?;
                Ok(Value::object(vm.alloc_variant(head.ptr(), index)))
            }
            RealizedTy::EnumVariant(head, name) => {
                let variant = reader.string()?;
                let index = super::variant_written_as(vm, *head, name, variant, self.plans.naming)
                    .ok_or(Stop)?;
                Ok(Value::object(vm.alloc_variant(head.ptr(), index)))
            }
            RealizedTy::Media(kind) => {
                let head = kind
                    .wrapper_class()
                    .and_then(|wrapper| vm.lang_head(wrapper))
                    .ok_or(Stop)?;
                self.media(vm, *kind, head)
            }
            RealizedTy::Union(members) => self.union(vm, members),
            RealizedTy::Literal(literal, _) => self.literal(vm, literal),
            // Every JSON value is a value of `unknown`.
            RealizedTy::Unknown => self.parser.value(vm),
            // An interface is not a class, and the rest have no JSON form.
            RealizedTy::Interface(..)
            | RealizedTy::Uint8Array
            | RealizedTy::Resource
            | RealizedTy::PromptAst
            | RealizedTy::Function { .. }
            | RealizedTy::Future(..)
            | RealizedTy::Never
            | RealizedTy::RustType
            | RealizedTy::Type => Err(Stop),
        }
    }

    // The arms of [`Self::value`] that recurse are functions of their own, so
    // each level of a deep document costs the stack only what it uses.

    fn list(&mut self, vm: &mut BexVm, element: &RealizedTy) -> Result<Value, Stop> {
        self.parser.reader.open_array()?;
        let mut items = Vec::new();
        while self.parser.reader.next_element(items.is_empty())? {
            items.push(self.value(vm, element)?);
        }
        Ok(Value::object(
            vm.alloc(Object::Array(Array::new(element.clone(), items))),
        ))
    }

    /// A recursive alias decodes as its definition.
    fn alias(&mut self, vm: &mut BexVm, ty: &RealizedTy) -> Result<Value, Stop> {
        let mut members = Vec::new();
        super::alias_members(vm, ty, &mut Vec::new(), &mut members);
        match members.as_slice() {
            [] => Err(Stop),
            [member] => self.value(vm, member),
            members => self.union(vm, members),
        }
    }

    /// The member that matches best ([`super::union`]), tried in order until
    /// one decodes.
    fn union(&mut self, vm: &mut BexVm, members: &[RealizedTy]) -> Result<Value, Stop> {
        let first = self.parser.reader.peek().ok_or(Stop)?;
        // `T?`, the common case: `null` is `null`, anything else is a `T`.
        if let Some(other) = super::union::other_than_null(members) {
            return if first == b'n' {
                self.parser.reader.null().map(|()| Value::NULL)
            } else {
                self.value(vm, other)
            };
        }
        let kind = Kind::of_text(first);
        let start = self.parser.reader.mark();
        let candidates = Candidates::of(vm, members, kind);
        // The keys are read ahead only when the order depends on them.
        let scanned = if candidates.counts_keys() {
            let keys = self.parser.reader.object_keys()?;
            self.parser.reader.reset(start);
            Some(keys)
        } else {
            None
        };
        let keys = scanned.as_deref().map(Keys::Text);
        for index in candidates.order(vm, members, self.plans, keys.as_ref()) {
            match self.value(vm, &members[index]) {
                Ok(value) => return Ok(value),
                Err(Stop) => self.parser.reader.reset(start),
            }
        }
        Err(Stop)
    }

    /// An integral number, or a string of decimal digits.
    fn bigint(&mut self) -> Result<num_bigint::BigInt, Stop> {
        let reader = &mut self.parser.reader;
        let bigint = match reader.peek() {
            Some(b'"') => super::parse_decimal_bigint(reader.string()?),
            _ => reader.number()?.as_bigint(),
        };
        bigint.ok_or(Stop)
    }

    fn literal(&mut self, vm: &mut BexVm, literal: &baml_type::Literal) -> Result<Value, Stop> {
        let reader = &mut self.parser.reader;
        match literal {
            baml_type::Literal::Bool(expected) => match reader.bool()? {
                actual if actual == *expected => Ok(Value::bool(actual)),
                _ => Err(Stop),
            },
            baml_type::Literal::String(expected) => {
                if reader.string()? != expected.as_str() {
                    return Err(Stop);
                }
                Ok(Value::object(vm.alloc_string(expected.as_str())))
            }
            baml_type::Literal::Int(expected) => match reader.number()?.as_i64() {
                Some(actual) if actual == *expected => Ok(Value::int(actual)),
                _ => Err(Stop),
            },
            baml_type::Literal::Float(expected) => {
                let actual = reader.number()?.as_f64().ok_or(Stop)?;
                match expected.parse::<f64>() {
                    Ok(expected) if (expected - actual).abs() < f64::EPSILON => {
                        Ok(Value::object(vm.alloc_float(actual)))
                    }
                    _ => Err(Stop),
                }
            }
            baml_type::Literal::Bigint(expected) => {
                let actual = self.bigint()?;
                if actual != *expected {
                    return Err(Stop);
                }
                vm.try_alloc_bigint(Arc::new(actual)).map_err(|_| Stop)
            }
        }
    }

    fn map(&mut self, vm: &mut BexVm, key: &RealizedTy, value: &RealizedTy) -> Result<Value, Stop> {
        if !baml_type::normalize::is_string_key(key.as_ty(), vm) {
            return Err(Stop);
        }
        // Any string is a `string` key; narrower key types check each one.
        let any_key = matches!(key, RealizedTy::String);
        self.parser.reader.open_object()?;
        let mut entries = Entries::default();
        while let Some(written) = self.parser.reader.next_key(entries.is_empty())? {
            if !any_key && !super::key_fits(vm, written, key) {
                return Err(Stop);
            }
            let written = BexStr::from(written);
            let decoded = self.value(vm, value)?;
            entries.insert(written, decoded);
        }
        Ok(entries.alloc(vm, key.clone(), value.clone()))
    }

    /// An instance of the class at `head`, read by its fields' JSON keys. A
    /// key the class does not declare is checked and skipped.
    fn class(
        &mut self,
        vm: &mut BexVm,
        head: TypeHead,
        type_args: &[RealizedTy],
    ) -> Result<Value, Stop> {
        self.parser.reader.open_object()?;
        let plan = self.plans.class(vm, head, type_args).map_err(|_| Stop)?;
        let mut slots: Vec<Option<Value>> = vec![None; plan.fields.len()];
        let mut first = true;
        while let Some(key) = self.parser.reader.next_key(first)? {
            first = false;
            let (field, others) = {
                let mut fields = plan.fields_named(key);
                let field = fields.next();
                (field, fields.collect::<Vec<usize>>())
            };
            let Some(field) = field else {
                self.parser.reader.skip_value()?;
                continue;
            };
            let start = self.parser.reader.mark();
            slots[field] = Some(self.value(vm, &plan.fields[field].ty)?);
            // Shared keys are rare: every field the key names reads the same
            // text, each as its own type.
            for other in others {
                let end = self.parser.reader.mark();
                self.parser.reader.reset(start);
                slots[other] = Some(self.value(vm, &plan.fields[other].ty)?);
                self.parser.reader.reset(end);
            }
        }
        let mut values = Vec::with_capacity(slots.len());
        for (field, slot) in plan.fields.iter().zip(slots) {
            let value = if !field.shown {
                super::super::sap::empty_value(vm, &field.ty, &mut Vec::new())
                    .ok()
                    .flatten()
                    .ok_or(Stop)?
            } else if let Some(value) = slot {
                value
            } else if field.may_be_absent {
                Value::NULL
            } else {
                return Err(Stop);
            };
            values.push(value);
        }
        Ok(Value::object(vm.alloc(Object::Instance(Instance::new(
            plan.class,
            plan.type_args.clone(),
            values,
        )))))
    }

    /// A media value from its tagged object.
    fn media(&mut self, vm: &mut BexVm, kind: MediaKind, head: TypeHead) -> Result<Value, Stop> {
        let reader = &mut self.parser.reader;
        reader.open_object()?;
        let mut fields = MediaFields::default();
        let mut first = true;
        while let Some(key) = reader.next_key(first)? {
            first = false;
            let slot = match key {
                "kind" => &mut fields.kind,
                "source" => &mut fields.source,
                "value" => &mut fields.value,
                "mime" => &mut fields.mime,
                "name" => &mut fields.name,
                _ => {
                    reader.skip_value()?;
                    continue;
                }
            };
            // A tag that is not a string reads as absent.
            *slot = if reader.peek() == Some(b'"') {
                Some(reader.string()?.to_owned())
            } else {
                reader.skip_value()?;
                None
            };
        }
        super::build_media(vm, &fields, kind, head).map_err(|_| Stop)
    }
}
