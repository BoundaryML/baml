//! What decoding needs to know about a class, worked out once per call
//! instead of once per instance.

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use bex_vm_types::{HeapPtr, Object, RealizedTy, TypeHead};
use rustc_hash::FxHashMap;
use sys_types::JsonNaming;

use crate::BexVm;

/// A class instantiated with its type arguments, as JSON reads it under one
/// naming.
#[derive(Clone)]
pub(super) struct ClassPlan {
    pub(super) head: TypeHead,
    pub(super) class: HeapPtr,
    pub(super) type_args: Box<[RealizedTy]>,
    /// Each field, in slot order.
    pub(super) fields: Vec<FieldPlan>,
    /// Two shown fields go by the same JSON key, so one key fills both.
    pub(super) shared_keys: bool,
}

#[derive(Clone)]
pub(super) struct FieldPlan {
    /// The JSON key under the plan's naming.
    pub(super) key: String,
    /// The key's map hash, to look it up in a `json` object; worked out the
    /// first time it is needed.
    hash: OnceLock<u64>,
    /// Whether JSON carries the field under the plan's naming: a `@skip`
    /// field under aliased names holds its type's empty value instead.
    pub(super) shown: bool,
    /// The field's type with the class's type arguments substituted.
    pub(super) ty: RealizedTy,
    /// A field that can be `null` may be absent, and then decodes to `null`.
    pub(super) may_be_absent: bool,
}

impl FieldPlan {
    /// The map hash of the field's JSON key.
    pub(super) fn hash(&self) -> u64 {
        *self
            .hash
            .get_or_init(|| bex_vm_types::map_string_hash(&self.key))
    }
}

impl ClassPlan {
    /// The class's name in messages.
    pub(super) fn named(&self) -> String {
        baml_type::HeadDisplay::head_display_name(&self.head)
    }

    /// The declared name of field `index`, for messages.
    pub(super) fn field_name(&self, vm: &BexVm, index: usize) -> String {
        match vm.get_object(self.class) {
            Object::Class(class) => class.fields[index].name.clone(),
            _ => self.fields[index].key.clone(),
        }
    }

    /// The plan for the class at `head` instantiated with `type_args`, or the
    /// class's name when `head` does not hold a class.
    fn new(
        vm: &BexVm,
        head: TypeHead,
        type_args: &[RealizedTy],
        naming: JsonNaming,
    ) -> Result<Self, String> {
        let class = head.ptr();
        let Object::Class(declaration) = vm.get_object(class) else {
            return Err(baml_type::HeadDisplay::head_display_name(&head));
        };
        let fields: Vec<FieldPlan> = declaration
            .fields
            .iter()
            .map(|field| {
                // Substitute class-level type-args into the field's template so
                // a `Container<User>::item` field decodes against `User` rather
                // than erased runtime metadata.
                let ty = vm.realize_field_ty(&field.field_template, type_args);
                let key = naming.key(&field.name, field.alias.as_deref()).to_owned();
                FieldPlan {
                    hash: OnceLock::new(),
                    key,
                    shown: naming.shows(field.skip),
                    may_be_absent: ty.is_nullable_union() || ty.is_null(),
                    ty,
                }
            })
            .collect();
        let shown: Vec<&str> = fields
            .iter()
            .filter(|field| field.shown)
            .map(|field| field.key.as_str())
            .collect();
        let shared_keys = shown
            .iter()
            .enumerate()
            .any(|(i, key)| shown[..i].contains(key));
        Ok(Self {
            head,
            class,
            type_args: type_args.into(),
            fields,
            shared_keys,
        })
    }

    /// The shown fields JSON names `key`: one, unless keys are shared.
    pub(super) fn fields_named<'a>(&'a self, key: &'a str) -> impl Iterator<Item = usize> + 'a {
        let mut found = false;
        self.fields
            .iter()
            .enumerate()
            .filter(move |(_, field)| {
                if found && !self.shared_keys {
                    return false;
                }
                let hit = field.shown && field.key == key;
                found |= hit;
                hit
            })
            .map(|(i, _)| i)
    }

    /// Repoint every heap pointer the plan holds after a collection.
    pub(super) fn forward(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        if let Some(moved) = forwarding.get(&self.class) {
            self.class = *moved;
            self.head.forward_to(*moved);
        }
        let forward_heads = |ty: &mut RealizedTy| {
            ty.visit_heads_mut(&mut |head| {
                if head.is_resolved()
                    && let Some(moved) = forwarding.get(&head.ptr())
                {
                    head.forward_to(*moved);
                }
            });
        };
        self.type_args.iter_mut().for_each(forward_heads);
        self.fields
            .iter_mut()
            .for_each(|field| forward_heads(&mut field.ty));
    }

    /// Every heap pointer the plan holds.
    pub(super) fn roots(&self, roots: &mut Vec<HeapPtr>) {
        roots.push(self.class);
        let mut visit = |ty: &RealizedTy| {
            ty.visit_heads(&mut |head| {
                if head.is_resolved() {
                    roots.push(head.ptr());
                }
            });
        };
        self.type_args.iter().for_each(&mut visit);
        self.fields.iter().for_each(|field| visit(&field.ty));
    }
}

/// The class plans one call has worked out, under its naming.
pub(super) struct Plans {
    pub(super) naming: JsonNaming,
    plans: FxHashMap<(TypeHead, Box<[RealizedTy]>), Arc<ClassPlan>>,
}

impl Plans {
    pub(super) fn new(naming: JsonNaming) -> Self {
        Self {
            naming,
            plans: FxHashMap::default(),
        }
    }

    /// The plan for the class at `head` with `type_args`, or the class's name
    /// when `head` does not hold a class.
    pub(super) fn class(
        &mut self,
        vm: &BexVm,
        head: TypeHead,
        type_args: &[RealizedTy],
    ) -> Result<Arc<ClassPlan>, String> {
        if let Some(plan) = self.plans.get(&(head, type_args.into())) {
            return Ok(Arc::clone(plan));
        }
        let plan = Arc::new(ClassPlan::new(vm, head, type_args, self.naming)?);
        self.plans
            .insert((head, type_args.into()), Arc::clone(&plan));
        Ok(plan)
    }

    /// Forget every plan: after a collection their pointers may be stale.
    pub(super) fn clear(&mut self) {
        self.plans.clear();
    }
}
