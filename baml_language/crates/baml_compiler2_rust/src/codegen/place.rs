//! Reads and writes locals, fields, elements, and captured cells.
//! Captured locals hold cells; dereferences access their values.

use baml_compiler2_mir::{BlockId, CellId, Operand, Place, RuntimeTy};
use bex_vm_types::{
    layout::{RawKind, SlotKind},
    types::list_descriptor,
};

use super::{Expr, Gen, Generator, unsupported};
use crate::{admit::Rejection, analysis::Repr, render::coerce};

/// A field of a base whose class the emitter knows, by the base's static
/// type: the field's slot.
struct KnownField {
    offset: u32,
    kind: SlotKind,
}

/// The `FieldBase` reader for a slot, and the value representation it
/// yields. Tagged slots go through the VM, which boxes floats.
fn slot_read(kind: SlotKind, offset: u32) -> (String, Repr) {
    match kind {
        SlotKind::Raw(RawKind::Int) => (format!("unsafe {{ f.int({offset}) }}"), Repr::Int),
        SlotKind::Raw(RawKind::Float) => (format!("unsafe {{ f.float({offset}) }}"), Repr::Float),
        SlotKind::Raw(RawKind::Bool) => (format!("unsafe {{ f.bool({offset}) }}"), Repr::Bool),
        SlotKind::Ptr => (format!("unsafe {{ f.ptr({offset}) }}"), Repr::Value),
        SlotKind::Tagged16 => (
            format!("unsafe {{ vm.load_tagged_at(f, {offset}) }}"),
            Repr::Value,
        ),
    }
}

/// Elements of a list whose element slot kind the emitter knows.
struct KnownElems {
    descriptor: u32,
    stride: u32,
    kind: SlotKind,
}

/// The element layout of a list of `element`, when the type is fully known.
fn elems_of(element: &RuntimeTy) -> Option<KnownElems> {
    let element = baml_compiler2_mir::RealizedTy::try_from(element).ok()?;
    let kind = SlotKind::of_type(&element);
    let stride = u32::try_from(kind.elem_kind().stride()).ok()?;
    Some(KnownElems {
        descriptor: list_descriptor(kind).0,
        stride,
        kind,
    })
}

/// The value representation a raw slot stores without a check, if any.
fn raw_repr(raw: RawKind) -> Repr {
    match raw {
        RawKind::Int => Repr::Int,
        RawKind::Float => Repr::Float,
        RawKind::Bool => Repr::Bool,
    }
}

impl<'db> Generator<'_, 'db> {
    pub(super) fn operand(&mut self, block: BlockId, operand: &Operand<'db>) -> Gen<Expr> {
        let expr = match operand {
            Operand::Copy(place) | Operand::Move(place) => self.load(block, place)?,
            Operand::Constant(constant) => self.constant(block, constant)?,
        };
        Ok(self.hoist(expr))
    }

    pub(super) fn load(&mut self, block: BlockId, place: &Place) -> Gen<Expr> {
        match place {
            Place::Local(local) => self.load_local(*local),
            Place::Field { base, field } => self.load_field(block, base, *field),
            Place::Index { base, index, kind } => self.load_index(block, base, *index, *kind),
            Place::Capture(index) => Ok(Expr::new(
                format!("vm.capture_cell(fr, {index})?"),
                Repr::Value,
            )),
            Place::Deref(cell) => {
                let (cell, repr) = self.cell(cell);
                let code = coerce(&format!("vm.cell_load({cell})?"), Repr::Value, repr);
                Ok(Expr::new(code, repr))
            }
        }
    }

    fn load_local(&mut self, local: baml_compiler2_mir::Local) -> Gen<Expr> {
        // Frame-derived types are rebuilt per read; static types are cached per site.
        if let Some(template) = self.analysis.deferred_types.get(&local.0) {
            return self.type_value(template);
        }
        if let Some(ty) = self.analysis.static_types.get(&local.0) {
            let code = ty.code.clone();
            let site = match self.static_value_sites.iter().position(|c| *c == code) {
                Some(site) => site,
                None => {
                    self.static_value_sites.push(code.clone());
                    self.static_value_sites.len() - 1
                }
            };
            return Ok(Expr::new(
                format!("vm.static_type_value(fr, {site}, {code})"),
                Repr::Value,
            ));
        }
        // The analysis keeps only iterators the loop alone consumes.
        if self.analysis.array_iters.contains(&local.0) {
            return Err(Rejection::Invalid("array iterator used as a value".into()));
        }
        Ok(Expr::atom(format!("l{}", local.0), self.repr(local)))
    }

    fn load_field(&mut self, block: BlockId, base: &Place, field: usize) -> Gen<Expr> {
        if let Some(known) = self.known_field(base, field) {
            return self.direct_load(block, base, field, &known);
        }
        let needs = self.analysis.escape.object_needs(base);
        let base = self.load(block, base)?;
        let base = self.value_arg(&base);
        let load = self.by_locality(
            needs,
            &format!("vm.load_field_local({base}, {field})"),
            &format!("vm.load_field({base}, {field})"),
        );
        let code = self.fallible(block, &load)?;
        Ok(Expr::new(code, Repr::Value))
    }

    fn load_index(
        &mut self,
        block: BlockId,
        base: &Place,
        index: baml_compiler2_mir::Local,
        kind: baml_compiler2_mir::IndexKind,
    ) -> Gen<Expr> {
        if !matches!(kind, baml_compiler2_mir::IndexKind::Array) {
            return unsupported("map index");
        }
        if let Some(known) = self.known_elems(base, index) {
            return self.direct_elem_load(block, base, index, &known);
        }
        let needs = self.analysis.escape.object_needs(base);
        let base = self.load(block, base)?;
        let base = self.value_arg(&base);
        let index = self.value_arg(&Expr::atom(format!("l{}", index.0), self.repr(index)));
        let load = self.by_locality(
            needs,
            &format!("vm.load_array_element_local({base}, {index})"),
            &format!("vm.load_array_element({base}, {index})"),
        );
        let code = self.fallible(block, &load)?;
        Ok(Expr::new(code, Repr::Value))
    }

    fn store_field(
        &mut self,
        block: BlockId,
        base: &Place,
        field: usize,
        value: &Expr,
    ) -> Gen<String> {
        if let Some(known) = self.known_field(base, field) {
            return self.direct_store(block, base, field, &known, value);
        }
        let needs = self.analysis.escape.object_needs(base);
        let value = self.value_arg(value);
        let base = self.load(block, base)?;
        let base = self.value_arg(&base);
        let store = self.by_locality(
            needs,
            &format!("vm.store_field_local({base}, {field}, {value})"),
            &format!("vm.store_field({base}, {field}, {value})"),
        );
        let code = self.fallible(block, &store)?;
        Ok(format!("{code};"))
    }

    fn store_index(
        &mut self,
        block: BlockId,
        base: &Place,
        index: baml_compiler2_mir::Local,
        kind: baml_compiler2_mir::IndexKind,
        value: &Expr,
    ) -> Gen<String> {
        if !matches!(kind, baml_compiler2_mir::IndexKind::Array) {
            return unsupported("map index");
        }
        if let Some(known) = self.known_elems(base, index) {
            return self.direct_elem_store(block, base, index, &known, value);
        }
        let needs = self.analysis.escape.object_needs(base);
        let value = self.value_arg(value);
        let base = self.load(block, base)?;
        let base = self.value_arg(&base);
        let index = self.value_arg(&Expr::atom(format!("l{}", index.0), self.repr(index)));
        let store = self.by_locality(
            needs,
            &format!("vm.store_array_element_local({base}, {index}, {value})"),
            &format!("vm.store_array_element({base}, {index}, {value})"),
        );
        let code = self.fallible(block, &store)?;
        Ok(format!("{code};"))
    }

    /// The slot of `field` when `base` is a local of a known class type.
    fn known_field(&self, base: &Place, field: usize) -> Option<KnownField> {
        let Place::Local(local) = base else {
            return None;
        };
        // A nullable base reaches a field access only where it is not null;
        // the access's class check takes the generic path when it is.
        let RuntimeTy::Class(name, _) = self.body.locals[local.0].ty.strip_null() else {
            return None;
        };
        let (class, _) = self.index.class_at(&name)?;
        let slot = self.index.field_slot(class, field)?;
        Some(KnownField {
            offset: slot.offset,
            kind: slot.kind,
        })
    }

    /// The bound class word of `class`, bound in the body's prologue.
    pub(super) fn class_word(&mut self, class: usize) -> String {
        self.class_words.insert(class);
        format!("k{class}")
    }

    /// A field read at its offset when the base is an instance of the known
    /// class, else the generic load; both in the slot's representation.
    fn direct_load(
        &mut self,
        block: BlockId,
        base: &Place,
        field: usize,
        known: &KnownField,
    ) -> Gen<Expr> {
        let base = self.load(block, base)?;
        let base = self.value_arg(&base);
        let (fast, repr) = slot_read(known.kind, known.offset);
        let slow = self.fallible(block, &format!("vm.load_field({base}, {field})"))?;
        let slow = coerce(&slow, Repr::Value, repr);
        Ok(Expr::new(
            format!(
                "(match unsafe {{ bex_vm::compiled_rt::FieldBase::typed({base}) }} {{ Some(f) => {fast}, None => {slow} }})"
            ),
            repr,
        ))
    }

    /// A field store at its offset when the base is an instance of the known
    /// class and the value needs no check for the slot, else the generic store.
    fn direct_store(
        &mut self,
        block: BlockId,
        base: &Place,
        field: usize,
        known: &KnownField,
        value: &Expr,
    ) -> Gen<String> {
        let value = self.hoist(Expr {
            code: value.code.clone(),
            repr: value.repr,
            atom: value.atom,
        });
        let base = self.load(block, base)?;
        let base = self.value_arg(&base);
        // Boxed only on the generic path: a float boxes by allocating.
        let boxed = coerce(&value.code, value.repr, Repr::Value);
        let slow = self.fallible(
            block,
            &format!("{{ let b = {boxed}; vm.store_field({base}, {field}, b) }}"),
        )?;
        let fast = self.slot_write(block, known.kind, known.offset, &value, &boxed, &slow)?;
        Ok(format!(
            "match unsafe {{ bex_vm::compiled_rt::FieldBase::typed({base}) }} {{ Some(f) => {{ {fast} }}, None => {{ {slow}; }} }}"
        ))
    }

    /// The element layout when `base` is a local of a list type whose element
    /// type is fully known and `index` is an `int` local.
    fn known_elems(&self, base: &Place, index: baml_compiler2_mir::Local) -> Option<KnownElems> {
        let Place::Local(local) = base else {
            return None;
        };
        let RuntimeTy::List(element) = &self.body.locals[local.0].ty else {
            return None;
        };
        if self.repr(index) != Repr::Int {
            return None;
        }
        elems_of(element)
    }

    /// The length of a list local whose element type is fully known, read in
    /// place; `None` for any other container, which the generic length
    /// takes.
    pub(super) fn known_len(&mut self, block: BlockId, place: &Place) -> Gen<Option<Expr>> {
        let Place::Local(local) = place else {
            return Ok(None);
        };
        let RuntimeTy::List(element) = &self.body.locals[local.0].ty else {
            return Ok(None);
        };
        let Some(known) = elems_of(element) else {
            return Ok(None);
        };
        let list = self.load(block, place)?;
        let list = self.value_arg(&list);
        let slow = self.fallible(block, &format!("vm.container_len({list})"))?;
        let slow = coerce(&slow, Repr::Value, Repr::Int);
        Ok(Some(Expr::new(
            format!(
                "(match bex_vm::compiled_rt::ListBase::of({list}, {}) {{ Some(l) => l.len(), None => {slow} }})",
                known.descriptor
            ),
            Repr::Int,
        )))
    }

    /// The element read of an array walk's step: element `i{iter}` of the
    /// list `l{iter}` when the walked list's element type is fully known,
    /// `None` for the generic walk.
    pub(super) fn walk_elem(&self, iter: usize) -> Option<(String, Repr)> {
        // The walk's setup (`xs.iter()`) names the list it walks.
        let array = self.body.blocks.iter().find_map(|block| {
            let (walked, array) = crate::analysis::array_iter_walk(
                self.db,
                self.body,
                self.analysis.arity,
                block.terminator.as_ref()?,
                &self.analysis.array_iters,
            )?;
            (walked == iter).then_some(array)
        })?;
        let RuntimeTy::List(element) = &self.body.locals[array].ty else {
            return None;
        };
        let known = elems_of(element)?;
        let (read, repr) = slot_read(known.kind, 0);
        let (descriptor, stride) = (known.descriptor, known.stride);
        Some((
            format!(
                "bex_vm::compiled_rt::ListBase::of(l{iter}, {descriptor}).and_then(|l| l.elem(i{iter}, {stride})).map(|f| {read})"
            ),
            repr,
        ))
    }

    /// The element at a non-negative, in-bounds index read in place, else the
    /// generic load (negative indices, errors).
    fn direct_elem_load(
        &mut self,
        block: BlockId,
        base: &Place,
        index: baml_compiler2_mir::Local,
        known: &KnownElems,
    ) -> Gen<Expr> {
        let base = self.load(block, base)?;
        let base = self.value_arg(&base);
        let (fast, repr) = slot_read(known.kind, 0);
        let boxed_index = self.value_arg(&Expr::atom(format!("l{}", index.0), Repr::Int));
        let slow = self.fallible(
            block,
            &format!("vm.load_array_element({base}, {boxed_index})"),
        )?;
        let slow = coerce(&slow, Repr::Value, repr);
        let (descriptor, stride) = (known.descriptor, known.stride);
        Ok(Expr::new(
            format!(
                "(match bex_vm::compiled_rt::ListBase::of({base}, {descriptor}).and_then(|l| l.elem(l{}, {stride})) {{ Some(f) => {fast}, None => {slow} }})",
                index.0
            ),
            repr,
        ))
    }

    /// An element store in place when the index is in bounds and the value
    /// needs no check for the slot, else the generic store.
    fn direct_elem_store(
        &mut self,
        block: BlockId,
        base: &Place,
        index: baml_compiler2_mir::Local,
        known: &KnownElems,
        value: &Expr,
    ) -> Gen<String> {
        let value = self.hoist(Expr {
            code: value.code.clone(),
            repr: value.repr,
            atom: value.atom,
        });
        let base = self.load(block, base)?;
        let base = self.value_arg(&base);
        let boxed = coerce(&value.code, value.repr, Repr::Value);
        let slow = self.fallible(
            block,
            &format!(
                "{{ let b = {boxed}; vm.store_array_element({base}, Value::int(l{}), b) }}",
                index.0
            ),
        )?;
        let fast = self.slot_write(block, known.kind, 0, &value, &boxed, &slow)?;
        let (descriptor, stride) = (known.descriptor, known.stride);
        Ok(format!(
            "match bex_vm::compiled_rt::ListBase::of({base}, {descriptor}).and_then(|l| l.elem(l{}, {stride})) {{ Some(f) => {{ {fast} }}, None => {{ {slow}; }} }}",
            index.0
        ))
    }

    /// The store of `value` (`boxed`, an expression of it as a `Value`) to the
    /// slot at `offset` of `f`, or `slow` when the value needs a check the
    /// slot does not make.
    fn slot_write(
        &mut self,
        block: BlockId,
        kind: SlotKind,
        offset: u32,
        value: &Expr,
        boxed: &str,
        slow: &str,
    ) -> Gen<String> {
        Ok(match kind {
            SlotKind::Raw(raw) if value.repr == raw_repr(raw) => {
                let setter = match raw {
                    RawKind::Int => "set_int",
                    RawKind::Float => "set_float",
                    RawKind::Bool => "set_bool",
                };
                format!("unsafe {{ f.{setter}({offset}, {}) }}", value.code)
            }
            SlotKind::Ptr if value.repr == Repr::Value => format!(
                "if {boxed}.is_null() || {boxed}.is_object() {{ unsafe {{ f.set_ptr({offset}, {boxed}) }} }} else {{ {slow}; }}"
            ),
            SlotKind::Tagged16 => {
                let store = self.fallible(
                    block,
                    &format!(
                        "{{ let t = {boxed}; unsafe {{ vm.store_tagged_at(f, {offset}, t) }} }}"
                    ),
                )?;
                format!("{store};")
            }
            SlotKind::Raw(_) => format!("{slow};"),
            SlotKind::Ptr => format!("{slow};"),
        })
    }

    fn cell(&mut self, cell: &CellId) -> (String, Repr) {
        match cell {
            CellId::Local(local) => (format!("l{}", local.0), self.analysis.values[local.0]),
            CellId::Capture(index) => {
                let cell = self.hoist(Expr::new(
                    format!("vm.capture_cell(fr, {index})?"),
                    Repr::Value,
                ));
                let repr = self
                    .analysis
                    .captures
                    .get(*index)
                    .copied()
                    .unwrap_or(Repr::Value);
                (cell.code, repr)
            }
        }
    }

    pub(super) fn store(&mut self, block: BlockId, place: &Place, value: &Expr) -> Gen<String> {
        match place {
            Place::Local(local) => {
                let code = coerce(&value.code, value.repr, self.repr(*local));
                Ok(format!("l{} = {code};", local.0))
            }
            Place::Field { base, field } => self.store_field(block, base, *field, value),
            Place::Index { base, index, kind } => {
                self.store_index(block, base, *index, *kind, value)
            }
            Place::Deref(cell) => {
                let value = self.value_arg(value);
                let (cell, _) = self.cell(cell);
                Ok(format!("vm.cell_store({cell}, {value})?;"))
            }
            Place::Capture(_) => Err(Rejection::Invalid(
                "store into a closure's capture cell".into(),
            )),
        }
    }
}
