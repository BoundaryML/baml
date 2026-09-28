//! Assembly: every object placed in layout order with its tag assigned, the
//! slots filled, the package tables and the init order written.

use std::collections::HashMap;

use baml_type::typetag::TypeTag;
use bex_vm_types::{
    ConstValue, DeclPath, GlobalIndex, Object, ObjectIndex, Program,
    types::{ProgramImplRule, ProgramMethodImpl, ProgramPackage},
};

use super::{
    LinkError, LinkPackageId, LinkSet, PerPackage,
    layout::{Bucket, TailPart},
    order::{LayoutOrder, Objects, Slots, tail_of},
    resolve::PathKey,
    space::{Imports, OperandSpace, TailSpace, UnitSpace, relocate, relocate_template_heads},
};

/// Everything resolved; builds the image.
pub(super) struct Assembler<'l, 'a> {
    pub(super) set: &'l LinkSet<'a>,
    pub(super) order: &'l LayoutOrder,
    pub(super) slots: &'l Slots,
    pub(super) objects: &'l Objects,
    /// Every package's object exports by path, to image index.
    pub(super) exports: HashMap<PathKey, usize>,
    pub(super) units: PerPackage<Imports>,
    pub(super) tails: PerPackage<Imports>,
}

impl Assembler<'_, '_> {
    pub(super) fn assemble(self) -> Result<Program, LinkError> {
        let mut program = Program::new();
        program.globals = vec![ConstValue::Null; self.slots.total];
        self.place(&mut program)?;

        // A package's executable identity is its position: set order.
        let mut packages: Vec<ProgramPackage> = self
            .set
            .packages
            .iter()
            .map(|package| ProgramPackage {
                name: package.name.clone(),
                edges: package
                    .edges
                    .iter()
                    .map(|edge| bex_vm_types::types::ProgramEdge {
                        name: edge.name.clone(),
                        target: edge.target.0,
                        kind: edge.kind,
                    })
                    .collect(),
                ..ProgramPackage::default()
            })
            .collect();
        for (id, package) in self.set.ids().zip(&mut packages) {
            self.fill_slots(id, &mut program, package)?;
            self.fill_tables(id, package);
            self.fill_record(id, package);
            self.fill_impl_rules(id, &program, package)?;
        }
        self.fill_tails(&mut program, &mut packages)?;
        for package in &mut packages {
            package.canonicalize_impl_rules();
        }
        program.packages = packages;
        program.root = self.set.root.0;
        Ok(program)
    }

    fn unit_space(&self, id: LinkPackageId) -> UnitSpace<'_> {
        let package = self.set.package(id);
        UnitSpace {
            name: &package.name,
            ordinal: id.0 as usize,
            unit: package.unit,
            objects: &self.objects.units[id],
            slots: &self.slots.units[id],
            imports: &self.units[id],
        }
    }

    fn tail_space(&self, id: LinkPackageId) -> TailSpace<'_> {
        TailSpace {
            name: self.set.name(id),
            ordinal: id.0 as usize,
            tail: tail_of(self.set, id),
            objects: &self.objects.tails[id],
            slots: &self.slots.tails[id],
            imports: &self.tails[id],
        }
    }

    /// Push every object in layout order, relocating its operands and heads
    /// and assigning each declaration the tag its image index is.
    fn place(&self, program: &mut Program) -> Result<(), LinkError> {
        for &id in self.order.members() {
            let space = self.unit_space(id);
            let placed = &space.objects.code;
            for bucket in Bucket::ALL {
                for (k, object) in bucket.objects(space.unit).iter().enumerate() {
                    if bucket == Bucket::Code && placed[k].is_shadow() {
                        continue;
                    }
                    let mut object = object.clone();
                    relocate(&mut object, &space)?;
                    assign_tag(&mut object, program.objects.len());
                    program.objects.push(object);
                }
            }
            for part in LayoutOrder::parts(self.set, id) {
                let space = self.tail_space(id);
                for k in part.objects(space.tail) {
                    if space.objects[k].is_shadow() {
                        continue;
                    }
                    let mut object = space.tail.objects[k].clone();
                    relocate(&mut object, &space)?;
                    debug_assert_eq!(program.objects.len(), space.objects[k].abs());
                    program.objects.push(object);
                }
            }
        }
        debug_assert_eq!(program.objects.len(), self.objects.total);
        Ok(())
    }

    /// Fill a unit's global slots and its package's slot map: each cell as an
    /// ordinal from the package's base in the program pool.
    fn fill_slots(
        &self,
        id: LinkPackageId,
        program: &mut Program,
        package: &mut ProgramPackage,
    ) -> Result<(), LinkError> {
        let name = self.set.name(id);
        package.slot_base = GlobalIndex::from_raw(self.slots.units[id].func_base);
        for (path, flat) in &self.set.package(id).unit.exports.globals {
            let slot = self.slots.units[id]
                .local(*flat as usize)
                .unwrap_or_else(|| unreachable!("the partition was validated"));
            match path {
                DeclPath::Function(_) | DeclPath::InterfaceBody(_) => {
                    let Some(&abs) = self.exports.get(&(id, path.clone())) else {
                        return Err(LinkError::invalid(format!(
                            "package `{name}` slots {path} but pools no object for it"
                        )));
                    };
                    program.globals[slot] = ConstValue::Object(ObjectIndex::from_raw(abs));
                }
                // A `let`'s slot is filled by its package's `$init`.
                DeclPath::Let(_) => {}
                _ => unreachable!("the partition was validated"),
            }
            debug_assert_eq!(
                slot,
                package.slot_base.raw() + *flat as usize,
                "a unit's slots are contiguous from its base"
            );
            package.globals.insert(path.clone(), *flat);
        }
        Ok(())
    }

    /// Fill a package's declaration tables from its unit's object exports.
    fn fill_tables(&self, id: LinkPackageId, package: &mut ProgramPackage) {
        for (path, _) in &self.set.package(id).unit.exports.objects {
            let abs = ObjectIndex::from_raw(self.exports[&(id, path.clone())]);
            let (table, item) = match path {
                DeclPath::Class(item) => (&mut package.classes, item),
                DeclPath::Enum(item) => (&mut package.enums, item),
                DeclPath::Interface(item) => (&mut package.interfaces, item),
                DeclPath::TypeAlias(item) => (&mut package.type_aliases, item),
                DeclPath::Function(_) | DeclPath::InterfaceBody(_) | DeclPath::Let(_) => continue,
            };
            table.insert(item.clone(), abs);
        }
    }

    /// Copy the package record in, resolving its exported callables.
    fn fill_record(&self, id: LinkPackageId, package: &mut ProgramPackage) {
        let link_package = self.set.package(id);
        package
            .exported_names
            .clone_from(&link_package.record.exported_names);
        package
            .interface_blob
            .clone_from(&link_package.record.interface_blob);
    }

    /// Resolve a unit's impl rules: the interface head through its operand
    /// space, each provided body through its code placement, and every head
    /// in the rule's templates into its assigned tag.
    fn fill_impl_rules(
        &self,
        id: LinkPackageId,
        program: &Program,
        package: &mut ProgramPackage,
    ) -> Result<(), LinkError> {
        let space = self.unit_space(id);
        let name = space.name;
        for rule in &space.unit.impl_rules {
            let interface_head = ObjectIndex::from_raw(space.object(rule.interface_head.raw())?);
            if !matches!(
                program.objects.get(interface_head.raw()),
                Some(Object::Interface(_))
            ) {
                return Err(LinkError::invalid(format!(
                    "package `{name}` implements an object that is not an interface"
                )));
            }
            let mut methods = indexmap::IndexMap::new();
            for (method, body) in &rule.methods {
                let abs = space
                    .objects
                    .code
                    .get(body.code_offset as usize)
                    .map(|placed| placed.abs())
                    .ok_or_else(|| {
                        LinkError::invalid(format!(
                            "package `{name}` impl method `{method}` references code offset {} \
                             outside its code bucket",
                            body.code_offset
                        ))
                    })?;
                // A provided body is never a named function; a corrupt or
                // stale unit fails the link instead of confusing the VM at
                // dispatch.
                if !matches!(
                    program.objects.get(abs),
                    Some(Object::Function(function)) if function.is_interface_body
                ) {
                    return Err(LinkError::invalid(format!(
                        "package `{name}` impl method `{method}` resolves to an object that is \
                         not an interface body"
                    )));
                }
                let mut frame = body.frame.clone();
                for template in &mut frame {
                    relocate_template_heads(template, &space)?;
                }
                methods.insert(
                    method.clone(),
                    ProgramMethodImpl {
                        fqn: ObjectIndex::from_raw(abs),
                        frame,
                    },
                );
            }
            let mut for_ty_pattern = rule.for_ty_pattern.clone();
            relocate_template_heads(&mut for_ty_pattern, &space)?;
            let mut generic_param_bounds = rule.generic_param_bounds.clone();
            for bound in generic_param_bounds.iter_mut().flatten() {
                bound.interface = space.head(&bound.interface)?;
                for arg in &mut bound.args {
                    relocate_template_heads(arg, &space)?;
                }
                for (_, assoc) in &mut bound.assoc {
                    relocate_template_heads(assoc, &space)?;
                }
            }
            let mut interface_args = rule.interface_args.clone();
            for arg in &mut interface_args {
                relocate_template_heads(arg, &space)?;
            }
            let mut interface_assoc = rule.interface_assoc.clone();
            for (_, assoc) in &mut interface_assoc {
                relocate_template_heads(assoc, &space)?;
            }
            package
                .impl_rules
                .entry(interface_head)
                .or_default()
                .push(ProgramImplRule {
                    interface_head,
                    for_ty_pattern,
                    generic_param_bounds,
                    interface_args,
                    interface_assoc,
                    methods,
                    field_links: rule.field_links.clone(),
                });
        }
        Ok(())
    }

    /// Fill every tail's slots and the package's structural init references.
    /// The init order lists the packages in initialization order, whatever
    /// their placement.
    fn fill_tails(
        &self,
        program: &mut Program,
        packages: &mut [ProgramPackage],
    ) -> Result<(), LinkError> {
        for &id in self.order.members() {
            for part in LayoutOrder::parts(self.set, id) {
                let space = self.tail_space(id);
                let slot_base = space.slots.base(part);
                let slots = part.slots(space.tail);
                for (ordinal, &object) in slots.iter().enumerate() {
                    program.globals[slot_base + ordinal] = ConstValue::Object(
                        ObjectIndex::from_raw(space.objects[object as usize].abs()),
                    );
                }
                let named = part
                    .named(space.tail)
                    .unwrap_or_else(|| unreachable!("ordered parts are named"));
                let Some(ordinal) = slots.iter().position(|&object| object == named) else {
                    return Err(LinkError::invalid(format!(
                        "package `{}` {} owns no tail slot",
                        space.name,
                        part.synthesized_name()
                    )));
                };
                let abs = space.objects[named as usize].abs();
                debug_assert_eq!(
                    program.globals[slot_base + ordinal],
                    ConstValue::Object(ObjectIndex::from_raw(abs)),
                    "the named part's slot holds its object"
                );
                let package = &mut packages[id.0 as usize];
                match part {
                    TailPart::Init => package.init = Some(ObjectIndex::from_raw(abs)),
                    TailPart::Test => package.test_init = Some(ObjectIndex::from_raw(abs)),
                }
            }
        }
        program.init_order = self.order.init_order().iter().map(|id| id.0).collect();
        debug_assert!(
            self.order
                .test_order()
                .iter()
                .all(|id| packages[id.0 as usize].test_init.is_some()),
            "every ordered test part was placed"
        );
        Ok(())
    }
}

/// A declaration's tag is its image index — the identity the linker owns.
fn assign_tag(object: &mut Object, abs: usize) {
    let tag = TypeTag::of_static_index(abs);
    match object {
        Object::Class(class) => class.type_tag = tag,
        Object::Enum(enm) => enm.type_tag = tag,
        Object::Interface(interface) => interface.type_tag = tag,
        Object::TypeAlias(alias) => alias.type_tag = tag,
        _ => {}
    }
}
