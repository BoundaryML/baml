//! Re-spelling the qualified type names a linked image mentions.
//!
//! The compiler names a runtime-compiled package's declarations exactly as it
//! names a static user file's: `class Item` becomes `user.Item`, whichever
//! package it was compiled into. That is fine for the compiler — one compile
//! sees one `Item` — but at runtime every compiled package's declarations land
//! in the same `user` namespace as every other's and as the static image's, and
//! a definition overlay keyed by qualified name can no longer say which `Item`
//! it holds.
//!
//! Once the VM mints a package it re-spells the package's *own* declarations
//! under `user.$dyn.<mint>.…` ([`QualifiedTypeName::to_runtime_local`]), which
//! makes the name a key again. That has to be all-or-nothing: a single
//! reference left at the old spelling would compare unequal to the renamed
//! declaration it names, so this module walks the linked image exhaustively
//! rather than the handful of places a bug happens to surface.
//!
//! Two things are deliberately *not* rewritten:
//!
//! - **`Class::type_tag`.** It is the digest of the emit-time name and is baked
//!   into this image's jump tables; recomputing it here would desynchronize the
//!   two. The tag was never package-discriminating (a static `user.Item` and
//!   every compiled package's `user.Item` already share one), and this change
//!   does not make it so.
//! - **`LocalName` keys** (`ProgramPackage::classes` and friends). Those index a
//!   package by the name its *source* used, which reflection (`get_class(
//!   "root.Item")`) and dependency linking both address it by. Lookups that
//!   arrive holding a minted qualified name go through
//!   [`QualifiedTypeName::source_namespace`] instead.
//!
//! The rest of a linked `Program` carries no qualified name at all: the
//! function/global index maps and `package_init_order` are keyed by *callable*
//! names, `client_metadata` and `test_cases` hold strings and values, and the
//! symbolic import/export tables were consumed by the linker before this runs.

use std::collections::HashSet;

use baml_type::{RenameTypeName, TypeName};

use crate::{
    ConstValue, Object, ObjectIndex, Program,
    types::{InterfaceBound, InterfaceDef, ProgramPackage},
};

/// Re-spell every local qualified name in `program` that names one of the
/// package's own declarations.
///
/// `owned` selects the object-pool entries this package actually defines;
/// imported entries are placeholders for another image's definitions and keep
/// the spelling their owner gave them.
///
/// Returns the re-spelled **class and enum** declarations paired with the pool
/// entry each names, for registration in the engine-wide dynamic-class table.
/// Interfaces are re-spelled with them but resolve through their own package's
/// interface table, so they are not reported.
pub fn rename_package_declarations(
    program: &mut Program,
    package: &baml_type::Name,
    owned: &HashSet<usize>,
    mint: u64,
) -> Vec<(TypeName, ObjectIndex)> {
    let Some(program_package) = program.packages.get(package) else {
        return Vec::new();
    };

    let mut declared: HashSet<TypeName> = HashSet::new();
    let mut minted = Vec::new();
    let declaration_indices = program_package
        .classes
        .values()
        .chain(program_package.enums.values())
        .chain(program_package.interfaces.values())
        .copied()
        .collect::<Vec<ObjectIndex>>();
    for index in declaration_indices {
        if !owned.contains(&index.raw()) {
            continue;
        }
        let (name, is_interface) = match program.objects.get(index.raw()) {
            Some(Object::Class(class)) => (&class.name, false),
            Some(Object::Enum(enm)) => (&enm.name, false),
            Some(Object::Interface(interface)) => (&interface.name, true),
            _ => continue,
        };
        if let Some(new) = name.to_runtime_local(mint) {
            declared.insert(name.clone());
            if !is_interface {
                minted.push((new, index));
            }
        }
    }
    if declared.is_empty() {
        return Vec::new();
    }

    // Keyed on the *old* spelling, which is the only thing a reference in this
    // image carries. A dependency that itself declares `Item` exports it under
    // the same `user.Item`, so if this package declares one too, both spellings
    // move here — but they were already one name to every by-name lookup in
    // this image, and `lookup_type` already answered both with this package's
    // declaration. What changes is that the two now compare *unequal*, which is
    // the truth: they are different definitions with different mints.
    let rename = move |name: &TypeName| {
        declared
            .contains(name)
            .then(|| name.to_runtime_local(mint))
            .flatten()
    };
    let rename: RenameTypeName<'_> = &rename;

    for (index, object) in program.objects.iter_mut().enumerate() {
        if owned.contains(&index) {
            rename_object(object, rename);
        }
    }
    if let Some(program_package) = program.packages.get_mut(package) {
        rename_program_package(program_package, rename);
    }
    minted
}

/// Rewrite every qualified name reachable from one pool object.
///
/// Exhaustive by construction: a new [`Object`] variant that carries a type
/// must be classified here or this stops compiling.
fn rename_object(object: &mut Object, rename: RenameTypeName<'_>) {
    match object {
        Object::Class(class) => {
            class.name = rename_name(&class.name, rename);
            for field in &mut class.fields {
                field.field_type = field.field_type.map_type_names(rename);
                field.field_template = field.field_template.map_type_names(rename);
            }
        }
        Object::Enum(enm) => enm.name = rename_name(&enm.name, rename),
        Object::Interface(interface) => rename_interface_def(interface, rename),
        // `display_param_types` / `display_return_type` are rendered at emit
        // from the source spelling, which is what a mint is masked back to
        // anyway — they are already what a user should see.
        Object::Function(function) => {
            function.return_type = function.return_type.map_type_names(rename);
            for param in &mut function.param_types {
                *param = param.map_type_names(rename);
            }
            function.throws_type = function.throws_type.map_type_names(rename);
            rename_bounds(&mut function.generic_param_bounds, rename);
            for constant in &mut function.bytecode.constants {
                rename_constant(constant, rename);
            }
        }
        Object::GenericFunction(function) => {
            for arg in &mut function.type_args {
                *arg = arg.map_type_names(rename);
            }
        }
        Object::Closure(closure) => {
            for arg in &mut closure.captured_type_args {
                *arg = arg.map_type_names(rename);
            }
        }
        Object::BoundMethod(method) => {
            for arg in &mut method.type_args {
                *arg = arg.map_type_names(rename);
            }
        }
        Object::Array(array) => {
            *array.element_ty = array.element_ty.map_type_names(rename);
        }
        Object::Map(map) => {
            *map.key_ty = map.key_ty.map_type_names(rename);
            *map.value_ty = map.value_ty.map_type_names(rename);
        }
        Object::Instance(instance) => {
            for arg in &mut instance.class_type_args {
                *arg = arg.map_type_names(rename);
            }
        }
        // None of these can reach a freshly-linked pool holding a stale
        // spelling. An impl rule is built from `ProgramPackage::impl_rules`
        // *after* the rename and a package object is allocated by the grafting
        // code itself, so both are already correct by the time they exist. The
        // two future kinds do carry types, but only `OpCode::Spawn` constructs
        // one — a compiled program has no way to emit either as a constant, and
        // a live `Future`'s types sit behind an `Arc` with no mutable accessor.
        // Everything else carries no type at all.
        Object::Package(_)
        | Object::ImplRule(_)
        | Object::Variant(_)
        | Object::HostClosure(_)
        | Object::Cell(_)
        | Object::String(_)
        | Object::Bigint(_)
        | Object::Uint8Array(_)
        | Object::Float(_)
        | Object::Future(_)
        | Object::UnscheduledFuture(_)
        | Object::RustData(_)
        | Object::Collector(_)
        | Object::Type(_) => {}
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(_) => {}
    }
}

fn rename_interface_def(interface: &mut InterfaceDef, rename: RenameTypeName<'_>) {
    interface.name = rename_name(&interface.name, rename);
    for (_, bounds) in &mut interface.args {
        for bound in bounds {
            *bound = bound.map_type_names(rename);
        }
    }
    for required in &mut interface.requires {
        *required = required.map_type_names(rename);
    }
    for (_, bound) in &mut interface.assoc {
        *bound = bound.map_type_names(rename);
    }
    for field in &mut interface.fields {
        field.ty = field.ty.map_type_names(rename);
    }
    for method in &mut interface.methods {
        for arg in &mut method.args {
            *arg = arg.map_type_names(rename);
        }
        for (_, arg) in &mut method.kwargs {
            *arg = arg.map_type_names(rename);
        }
        method.returns = method.returns.map_type_names(rename);
        method.errors = method.errors.map_type_names(rename);
    }
}

fn rename_program_package(package: &mut ProgramPackage, rename: RenameTypeName<'_>) {
    for rules in package.impl_rules.values_mut() {
        for rule in rules {
            rule.for_ty_pattern = rule.for_ty_pattern.map_type_names(rename);
            rename_bounds(&mut rule.generic_param_bounds, rename);
            for arg in &mut rule.interface_args {
                *arg = arg.map_type_names(rename);
            }
            for (_, assoc) in &mut rule.interface_assoc {
                *assoc = assoc.map_type_names(rename);
            }
            for method in rule.methods.values_mut() {
                for slot in &mut method.frame {
                    *slot = slot.map_type_names(rename);
                }
            }
        }
    }
    for alias in package.recursive_type_aliases.values_mut() {
        *alias = alias.map_type_names(rename);
    }
}

fn rename_bounds(bounds: &mut [Vec<InterfaceBound>], rename: RenameTypeName<'_>) {
    for slot in bounds {
        for bound in slot {
            bound.interface = rename_name(&bound.interface, rename);
            for arg in &mut bound.args {
                *arg = arg.map_type_names(rename);
            }
            for (_, assoc) in &mut bound.assoc {
                *assoc = assoc.map_type_names(rename);
            }
        }
    }
}

fn rename_constant(constant: &mut ConstValue, rename: RenameTypeName<'_>) {
    match constant {
        ConstValue::Type(template) => *template = template.map_type_names(rename),
        ConstValue::ClassWithTypeArgs {
            class_obj: _,
            type_args_templates,
        } => {
            for template in type_args_templates {
                *template = template.map_type_names(rename);
            }
        }
        ConstValue::OmittedArg
        | ConstValue::Null
        | ConstValue::Int(_)
        | ConstValue::Float(_)
        | ConstValue::Bool(_)
        | ConstValue::Object(_)
        | ConstValue::Literal(_) => {}
    }
}

fn rename_name(name: &TypeName, rename: RenameTypeName<'_>) -> TypeName {
    rename(name).unwrap_or_else(|| name.clone())
}

#[cfg(test)]
mod tests {
    use baml_type::{Name, RuntimeTy, TyAttr, TyTemplate, TypeName};
    use indexmap::IndexMap;

    use super::*;
    use crate::types::{Class, ClassField, LocalName, ProgramPackage};

    fn item_name() -> TypeName {
        TypeName::local(Name::new("Item"))
    }

    fn class_object(name: TypeName, field_ty: RuntimeTy) -> Object {
        Object::Class(Box::new(Class {
            name,
            fields: vec![ClassField {
                name: "next".to_string(),
                field_type: field_ty.clone(),
                field_template: TyTemplate::from(
                    baml_type::RealizedTy::try_from(&baml_type::Ty::from(field_ty))
                        .expect("the fixture field type is realized"),
                ),
                description: None,
                alias: None,
                docstring: None,
                other: IndexMap::new(),
                skip: false,
                runtime_type: None,
            }],
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            type_tag: 1234,
            ty_attr: TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
            runtime_type: None,
        }))
    }

    fn program_with_self_referential_item() -> Program {
        let field_ty = RuntimeTy::List(
            Box::new(RuntimeTy::Class(item_name(), Vec::new(), TyAttr::default())),
            TyAttr::default(),
        );
        let mut program = Program {
            objects: crate::ObjectPool::from_vec(vec![class_object(item_name(), field_ty)]),
            ..Program::default()
        };
        let mut package = ProgramPackage::default();
        package.classes.insert(
            LocalName {
                namespace: Vec::new(),
                name: Name::new("Item"),
            },
            ObjectIndex::from_raw(0),
        );
        package.recursive_type_aliases.insert(
            LocalName {
                namespace: Vec::new(),
                name: Name::new("Chain"),
            },
            RuntimeTy::Class(item_name(), Vec::new(), TyAttr::default()),
        );
        program.packages.insert(Name::new("user"), package);
        program
    }

    /// The declaration, the reference nested in its own field type, the field
    /// *template* built alongside it, and the alias that names it all move to
    /// the minted spelling together. Leaving any one behind would make the type
    /// compare unequal to the definition it names.
    #[test]
    fn every_mention_of_a_declaration_moves_together() {
        let mut program = program_with_self_referential_item();
        let owned = HashSet::from([0usize]);
        let minted = rename_package_declarations(&mut program, &Name::new("user"), &owned, 7);

        assert_eq!(minted.len(), 1);
        assert!(minted[0].0.has_runtime_mint(7));
        assert_eq!(minted[0].1, ObjectIndex::from_raw(0));

        let Some(Object::Class(class)) = program.objects.first() else {
            panic!("object 0 stays a class");
        };
        assert!(class.name.has_runtime_mint(7));
        // The tag is the emit-time digest and is deliberately left alone.
        assert_eq!(class.type_tag, 1234);

        let RuntimeTy::List(inner, _) = &class.fields[0].field_type else {
            panic!("field type stays a list");
        };
        assert!(matches!(inner.as_ref(), RuntimeTy::Class(name, ..) if name.has_runtime_mint(7)));
        let TyTemplate::List(inner, _) = &class.fields[0].field_template else {
            panic!("field template stays a list");
        };
        assert!(matches!(inner.as_ref(), TyTemplate::Class(name, ..) if name.has_runtime_mint(7)));

        let package = &program.packages[&Name::new("user")];
        let alias = package
            .recursive_type_aliases
            .values()
            .next()
            .expect("the alias survives");
        assert!(matches!(alias, RuntimeTy::Class(name, ..) if name.has_runtime_mint(7)));

        // The `LocalName` key is how reflection and dependency linking address
        // the declaration, so it keeps the source spelling.
        assert!(package.classes.contains_key(&LocalName {
            namespace: Vec::new(),
            name: Name::new("Item"),
        }));
    }

    /// An imported object belongs to whichever image defined it; re-spelling it
    /// here would rename another package's declaration.
    #[test]
    fn an_imported_object_is_left_at_its_owners_spelling() {
        let mut program = program_with_self_referential_item();
        let minted =
            rename_package_declarations(&mut program, &Name::new("user"), &HashSet::new(), 7);
        assert!(minted.is_empty());
        let Some(Object::Class(class)) = program.objects.first() else {
            panic!("object 0 stays a class");
        };
        assert_eq!(class.name, item_name());
    }

    /// A declaration is only reachable under the mint that made it. Two
    /// packages that each declare `Item` therefore produce two keys.
    #[test]
    fn two_mints_of_one_declaration_are_distinct() {
        let mut first = program_with_self_referential_item();
        let mut second = program_with_self_referential_item();
        let owned = HashSet::from([0usize]);
        let a = rename_package_declarations(&mut first, &Name::new("user"), &owned, 1);
        let b = rename_package_declarations(&mut second, &Name::new("user"), &owned, 2);
        assert_ne!(a, b);
        assert_eq!(a[0].0.render_user_facing(), b[0].0.render_user_facing());
    }

    /// Only the named package's declarations move: a type belonging to another
    /// package keeps its own spelling even when it is mentioned here.
    #[test]
    fn a_foreign_reference_inside_an_owned_object_is_untouched() {
        let foreign = TypeName::new(Name::new("dep"), Vec::new(), Name::new("Item"));
        let mut program = Program {
            objects: crate::ObjectPool::from_vec(vec![class_object(
                item_name(),
                RuntimeTy::Class(foreign.clone(), Vec::new(), TyAttr::default()),
            )]),
            ..Program::default()
        };
        let mut package = ProgramPackage::default();
        package.classes.insert(
            LocalName {
                namespace: Vec::new(),
                name: Name::new("Item"),
            },
            ObjectIndex::from_raw(0),
        );
        program.packages.insert(Name::new("user"), package);

        rename_package_declarations(
            &mut program,
            &Name::new("user"),
            &HashSet::from([0usize]),
            7,
        );
        let Some(Object::Class(class)) = program.objects.first() else {
            panic!("object 0 stays a class");
        };
        assert!(class.name.has_runtime_mint(7));
        assert!(
            matches!(&class.fields[0].field_type, RuntimeTy::Class(name, ..) if *name == foreign)
        );
    }
}
