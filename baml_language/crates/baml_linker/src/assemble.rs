//! Assembly: every object placed in layout order with its tag assigned, the
//! slots filled, the rendered views and package tables written.

use std::collections::{HashMap, HashSet};

use baml_base::Name;
use baml_type::typetag::TypeTag;
use bex_vm_types::{
    ConstValue, DeclPath, FnPath, GlobalIndex, Object, ObjectIndex, Program,
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

/// The rendered name views, with collision detection.
#[derive(Default)]
#[deprecated(
    note = "a rendered view of the flat name maps, kept only while the loader and engine still start from them; derived at load or deleted with the maps"
)]
struct Views {
    claimed: HashSet<String>,
}

impl Views {
    fn claim(&mut self, name: String) -> Result<String, LinkError> {
        if self.claimed.insert(name.clone()) {
            Ok(name)
        } else {
            Err(LinkError::DuplicateLinkName(name))
        }
    }
}

/// Rendered form of a declaration for the flat name maps: `pkg.ns.name` for
/// a free function or `let`, `pkg.ns.Class.name` for a method. Interface
/// bodies and types have no entry there.
#[deprecated(
    note = "a rendered view of the flat name maps, kept only while the loader and engine still start from them; derived at load or deleted with the maps"
)]
fn rendered_link_name(package: &Name, path: &DeclPath) -> Option<String> {
    let (namespace, tail): (&[Name], Vec<&str>) = match path {
        DeclPath::Function(FnPath::Free(item)) | DeclPath::Let(item) => {
            (&item.namespace, vec![item.name.as_str()])
        }
        DeclPath::Function(FnPath::Method { class, name }) => {
            (&class.namespace, vec![class.name.as_str(), name.as_str()])
        }
        DeclPath::Class(_)
        | DeclPath::Enum(_)
        | DeclPath::Interface(_)
        | DeclPath::TypeAlias(_)
        | DeclPath::InterfaceBody(_) => return None,
    };
    let parts = std::iter::once(package.as_str())
        .chain(namespace.iter().map(Name::as_str))
        .chain(tail);
    Some(parts.collect::<Vec<_>>().join("."))
}

/// The rendered name of a package's synthesized `$init` / `$init_test`: bare
/// for the local (workspace) package, package-prefixed otherwise.
#[deprecated(
    note = "rendered `$init` order: the loader runs `ProgramPackage::init` in package order instead, then it is deleted"
)]
fn synthesized_name(package: &Name, base: &str) -> String {
    match baml_type::Package::from_name(package.clone()) {
        baml_type::Package::Local => base.to_string(),
        baml_type::Package::Dep(_) => format!("{package}.{base}"),
    }
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
                ..ProgramPackage::default()
            })
            .collect();
        let mut views = Views::default();
        for (id, package) in self.set.ids().zip(&mut packages) {
            self.fill_slots(id, &mut program, package, &mut views)?;
            self.fill_tables(id, package);
            self.fill_record(id, package)?;
            self.fill_impl_rules(id, &program, package)?;
        }
        self.fill_tails(&mut program, &mut packages, &mut views)?;
        for package in &mut packages {
            package.canonicalize_impl_rules();
        }
        program.packages = packages;
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

    /// Fill a unit's global slots, its rendered views, and its package's own
    /// slot table.
    fn fill_slots(
        &self,
        id: LinkPackageId,
        program: &mut Program,
        package: &mut ProgramPackage,
        views: &mut Views,
    ) -> Result<(), LinkError> {
        let name = self.set.name(id);
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
                    if let Some(rendered) = rendered_link_name(name, path) {
                        let rendered = views.claim(rendered)?;
                        program.function_indices.insert(rendered.clone(), abs);
                        program.function_global_indices.insert(rendered, slot);
                    }
                }
                DeclPath::Let(_) => {
                    let rendered = rendered_link_name(name, path)
                        .unwrap_or_else(|| unreachable!("a let renders"));
                    program
                        .let_global_indices
                        .insert(views.claim(rendered)?, slot);
                }
                _ => unreachable!("the partition was validated"),
            }
            if !matches!(path, DeclPath::InterfaceBody(_)) {
                package
                    .globals
                    .insert(path.clone(), GlobalIndex::from_raw(slot));
            }
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
    fn fill_record(
        &self,
        id: LinkPackageId,
        package: &mut ProgramPackage,
    ) -> Result<(), LinkError> {
        let link_package = self.set.package(id);
        package
            .exported_names
            .clone_from(&link_package.record.exported_names);
        package
            .interface_blob
            .clone_from(&link_package.record.interface_blob);
        for (local, function) in &link_package.record.functions {
            let path = DeclPath::Function(function.clone());
            let Some(&abs) = self.exports.get(&(id, path.clone())) else {
                return Err(LinkError::UnresolvedImport {
                    package: link_package.name.clone(),
                    path,
                });
            };
            package
                .functions
                .insert(local.clone(), ObjectIndex::from_raw(abs));
        }
        Ok(())
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

    /// Fill every tail's slots, its `$init` / `$init_test` views, and the
    /// package's structural init references. The rendered init order lists
    /// the packages in initialization order, whatever their placement.
    fn fill_tails(
        &self,
        program: &mut Program,
        packages: &mut [ProgramPackage],
        views: &mut Views,
    ) -> Result<(), LinkError> {
        let mut init_names: HashMap<LinkPackageId, String> = HashMap::new();
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
                let rendered =
                    views.claim(synthesized_name(space.name, part.synthesized_name()))?;
                program.function_indices.insert(rendered.clone(), abs);
                program
                    .function_global_indices
                    .insert(rendered.clone(), slot_base + ordinal);
                let package = &mut packages[id.0 as usize];
                match part {
                    TailPart::Init => {
                        package.init = Some(ObjectIndex::from_raw(abs));
                        init_names.insert(id, rendered);
                    }
                    TailPart::Test => package.test_init = Some(ObjectIndex::from_raw(abs)),
                }
            }
        }
        for id in self.order.init_order() {
            program.package_init_order.push(
                init_names
                    .remove(id)
                    .unwrap_or_else(|| unreachable!("every ordered init part was placed")),
            );
        }
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
