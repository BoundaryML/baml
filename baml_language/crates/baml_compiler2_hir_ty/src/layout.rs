//! Layout facts of a DECLARATION, whichever lane declares it.
//!
//! A class's field order, an enum's variant order, an interface's member
//! table and an alias's value are properties of the declaration, and the
//! bytecode bakes them (`Place::Field { field }`, enum discriminants,
//! `VirtualFieldAccess { field_index }`). Before this module MIR and emit
//! each recomputed them from the two lanes — item data for source, rows for
//! a served package — into maps keyed by the WIRE spelling of the head, with
//! a third road that read a served class's link stub. Here every fact is
//! answered ONCE, keyed by the declaration's identity ([`ClassRef`],
//! [`EnumRef`], [`InterfaceRef`], [`AliasRef`]): the `Source` arm reads the
//! item's lowered data, the `External` arm reads its exported row, and no
//! consumer can hold a layout the declaration does not have.
//!
//! The lane a ROOT is served on decides which arm an enumerator mints — the
//! same law as [`crate::facts::definition_of`]: a root served from its
//! interface has rows, not items, whatever files it happens to carry.

use baml_base::{Name, SourceRoot};
use baml_compiler2_hir::{
    contributions::Definition, loc::DeclRef, package::is_served_from_interface,
};
use baml_type::{DeclName, Interface, ParamTy, Ty};

use crate::extern_loc::{
    AliasRef, ClassRef, EnumRef, InterfaceRef, extern_alias_loc, extern_alias_row,
    extern_class_loc, extern_class_row, extern_enum_loc, extern_enum_row, extern_interface_loc,
    extern_interface_row, mounted_alias_loc, mounted_class_loc, mounted_enum_loc,
    mounted_interface_loc,
};

// ── Head → ref ───────────────────────────────────────────────────────────────
//
// A head names a declaration on exactly one lane: the source item its package
// declares, or the row its package exports. `None` means the head names no
// declaration of that kind this database can see — never "not yet".

fn source_definition<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    head: &DeclName,
) -> Option<Definition<'db>> {
    crate::facts::definition_of(db, head)
}

/// The class `head` names, wherever it is declared.
pub fn class_ref_of<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    head: &DeclName,
) -> Option<ClassRef<'db>> {
    match source_definition(db, head) {
        Some(Definition::Class(class)) => Some(DeclRef::Source(class)),
        Some(_) => None,
        None => mounted_class_loc(db, head).map(DeclRef::External),
    }
}

/// The class a language package declares at `namespace.name`, whichever
/// lane serves the package — the class twin of
/// [`crate::callable::lang_function`]. `None` when the package is not
/// installed or declares no such class.
pub fn lang_class<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    package: baml_base::LangPackage,
    namespace: &[&str],
    name: &str,
) -> Option<ClassRef<'db>> {
    let root = baml_compiler2_hir::package::lang_roots(db).get(package)?;
    let namespace: Vec<Name> = namespace.iter().copied().map(Name::new).collect();
    let head = DeclName::in_root(root, namespace, Name::new(name));
    class_ref_of(db, &head)
}

/// The enum `head` names, wherever it is declared.
pub fn enum_ref_of<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    head: &DeclName,
) -> Option<EnumRef<'db>> {
    match source_definition(db, head) {
        Some(Definition::Enum(enum_loc)) => Some(DeclRef::Source(enum_loc)),
        Some(_) => None,
        None => mounted_enum_loc(db, head).map(DeclRef::External),
    }
}

/// The interface `head` names, wherever it is declared.
pub fn interface_ref_of<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    head: &DeclName,
) -> Option<InterfaceRef<'db>> {
    match source_definition(db, head) {
        Some(Definition::Interface(interface)) => Some(DeclRef::Source(interface)),
        Some(_) => None,
        None => mounted_interface_loc(db, head).map(DeclRef::External),
    }
}

/// The type alias `head` names, wherever it is declared.
pub fn alias_ref_of<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    head: &DeclName,
) -> Option<AliasRef<'db>> {
    match source_definition(db, head) {
        Some(Definition::TypeAlias(alias)) => Some(DeclRef::Source(alias)),
        Some(_) => None,
        None => mounted_alias_loc(db, head).map(DeclRef::External),
    }
}

// ── Ref → head ───────────────────────────────────────────────────────────────

/// The head a class declaration is spelled by.
pub fn class_head<'db>(db: &'db dyn baml_compiler2_hir::Db, class: ClassRef<'db>) -> DeclName {
    match class {
        DeclRef::Source(class) => crate::lower::class_qualified_name(db, class),
        DeclRef::External(class) => class.head(db).clone(),
    }
}

/// The head an enum declaration is spelled by.
pub fn enum_head<'db>(db: &'db dyn baml_compiler2_hir::Db, enum_ref: EnumRef<'db>) -> DeclName {
    match enum_ref {
        DeclRef::Source(enum_loc) => crate::lower::qualify_def(
            db,
            Definition::Enum(enum_loc),
            &baml_compiler2_hir::item_data::enum_data(db, enum_loc).name,
        ),
        DeclRef::External(enum_loc) => enum_loc.head(db).clone(),
    }
}

/// The head an interface declaration is spelled by.
pub fn interface_head<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    interface: InterfaceRef<'db>,
) -> DeclName {
    match interface {
        DeclRef::Source(interface) => crate::lower::interface_qualified_name(db, interface),
        DeclRef::External(interface) => interface.head(db).clone(),
    }
}

/// The head a type alias declaration is spelled by.
pub fn alias_head<'db>(db: &'db dyn baml_compiler2_hir::Db, alias: AliasRef<'db>) -> DeclName {
    match alias {
        DeclRef::Source(alias) => crate::lower::qualify_def(
            db,
            Definition::TypeAlias(alias),
            &baml_compiler2_hir::item_data::type_alias_data(db, alias).name,
        ),
        DeclRef::External(alias) => alias.head(db).clone(),
    }
}

// ── Enumerators ──────────────────────────────────────────────────────────────
//
// Every declaration of a kind a root declares, on the lane the root is served
// on, in the root's own declaration order (source: namespace, then item
// order; a served root: its interface's row order — the order its export
// walked, which is the same thing). Lazy: both lanes walk tracked,
// `returns(ref)` data, so nothing is materialized and a caller that stops
// early pays for what it read.

/// The iterator one lane yields — the two lanes' iterators are different
/// types, and a declaration walk is one or the other, never both.
enum Lane<S, X> {
    Source(S),
    Served(X),
}

impl<T, S: Iterator<Item = T>, X: Iterator<Item = T>> Iterator for Lane<S, X> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        match self {
            Lane::Source(source) => source.next(),
            Lane::Served(served) => served.next(),
        }
    }
}

/// The declarations of one kind `root` declares: `source` picks the kind
/// among its items, `served` among its rows, and `mint` is the kind's mint
/// (total here — the row was just enumerated from this interface).
fn package_declarations<'db, L: 'db, E: 'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    root: SourceRoot,
    source: fn(Definition<'db>) -> Option<L>,
    served: fn(&crate::package_interface::ExportedType) -> bool,
    mint: fn(&'db dyn baml_compiler2_hir::Db, &DeclName) -> Option<E>,
) -> impl Iterator<Item = DeclRef<L, E>> + 'db {
    if is_served_from_interface(db, root) {
        let interface = crate::package_interface::package_interface(db, root);
        return Lane::Served(interface.types.iter().flat_map(move |(namespace, rows)| {
            rows.iter()
                .filter(move |(_, row)| served(row))
                .map(move |(name, _)| {
                    let head = DeclName::in_root(root, namespace.clone(), name.clone());
                    DeclRef::External(mint(db, &head).unwrap_or_else(|| {
                        unreachable!("the row was enumerated from this interface")
                    }))
                })
        }));
    }
    Lane::Source(
        baml_compiler2_hir::package::package_items(db, root)
            .namespaces
            .values()
            .flat_map(|namespace| namespace.types.values().copied())
            .filter_map(source)
            .map(DeclRef::Source),
    )
}

/// Every class `root` declares.
pub fn package_classes(
    db: &dyn baml_compiler2_hir::Db,
    root: SourceRoot,
) -> impl Iterator<Item = ClassRef<'_>> + '_ {
    use crate::package_interface::ExportedType;
    package_declarations(
        db,
        root,
        |def| match def {
            Definition::Class(class) => Some(class),
            _ => None,
        },
        |row| matches!(row, ExportedType::Class { .. }),
        extern_class_loc,
    )
}

/// Every enum `root` declares.
pub fn package_enums(
    db: &dyn baml_compiler2_hir::Db,
    root: SourceRoot,
) -> impl Iterator<Item = EnumRef<'_>> + '_ {
    use crate::package_interface::ExportedType;
    package_declarations(
        db,
        root,
        |def| match def {
            Definition::Enum(enum_loc) => Some(enum_loc),
            _ => None,
        },
        |row| matches!(row, ExportedType::Enum { .. }),
        extern_enum_loc,
    )
}

/// Every interface `root` declares.
pub fn package_interfaces(
    db: &dyn baml_compiler2_hir::Db,
    root: SourceRoot,
) -> impl Iterator<Item = InterfaceRef<'_>> + '_ {
    use crate::package_interface::ExportedType;
    package_declarations(
        db,
        root,
        |def| match def {
            Definition::Interface(interface) => Some(interface),
            _ => None,
        },
        |row| matches!(row, ExportedType::Interface { .. }),
        extern_interface_loc,
    )
}

/// Every type alias `root` declares.
pub fn package_aliases(
    db: &dyn baml_compiler2_hir::Db,
    root: SourceRoot,
) -> impl Iterator<Item = AliasRef<'_>> + '_ {
    use crate::package_interface::ExportedType;
    package_declarations(
        db,
        root,
        |def| match def {
            Definition::TypeAlias(alias) => Some(alias),
            _ => None,
        },
        |row| matches!(row, ExportedType::TypeAlias { .. }),
        extern_alias_loc,
    )
}

// ── Class layout ─────────────────────────────────────────────────────────────
//
// Field ORDER is the layout: slot `i` of an instance is field `i` of the
// declaration. The `Source` arm reads `resolve_class_fields`, the one lowered
// view of the declared field list; the `External` arm reads the row, whose
// field list is the exporter's walk of the same declaration.

/// The class's fields in declaration (= slot) order, each typed in the
/// class's own generic frame ([`class_generic_params`]).
pub fn class_fields<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    class: ClassRef<'db>,
) -> Vec<(Name, Ty)> {
    match class {
        DeclRef::Source(class) => crate::lower::resolve_class_fields(db, class)
            .iter()
            .map(|(name, ty, _attrs)| (name.clone(), ty.clone()))
            .collect(),
        DeclRef::External(class) => extern_class_row(db, class)
            .fields
            .iter()
            .map(|(name, ty, _attrs)| (name.clone(), ty.clone()))
            .collect(),
    }
}

/// How many slots an instance of `class` has.
pub fn class_field_count<'db>(db: &'db dyn baml_compiler2_hir::Db, class: ClassRef<'db>) -> usize {
    match class {
        DeclRef::Source(class) => crate::lower::resolve_class_fields(db, class).len(),
        DeclRef::External(class) => extern_class_row(db, class).fields.len(),
    }
}

/// The slot of the field named `field`, if the class declares one.
pub fn class_field_index<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    class: ClassRef<'db>,
    field: &Name,
) -> Option<u32> {
    let index = match class {
        DeclRef::Source(class) => crate::lower::resolve_class_fields(db, class)
            .iter()
            .position(|(name, ..)| name == field)?,
        DeclRef::External(class) => extern_class_row(db, class)
            .fields
            .iter()
            .position(|(name, ..)| name == field)?,
    };
    Some(u32::try_from(index).expect("class field count fits u32"))
}

/// The declared type of the field named `field`, in the class's own generic
/// frame, if the class declares one.
pub fn class_field_ty<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    class: ClassRef<'db>,
    field: &Name,
) -> Option<Ty> {
    match class {
        DeclRef::Source(class) => crate::lower::resolve_class_fields(db, class)
            .iter()
            .find(|(name, ..)| name == field)
            .map(|(_, ty, _)| ty.clone()),
        DeclRef::External(class) => extern_class_row(db, class)
            .fields
            .iter()
            .find(|(name, ..)| name == field)
            .map(|(_, ty, _)| ty.clone()),
    }
}

/// The class's generic frame: the parameters its field types are written
/// over, in declaration order.
pub fn class_generic_params<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    class: ClassRef<'db>,
) -> Vec<ParamTy> {
    match class {
        DeclRef::Source(class) => crate::lower::class_generic_frame(db, class),
        DeclRef::External(class) => extern_class_row(db, class).generic_params.to_vec(),
    }
}

// ── Enum layout ──────────────────────────────────────────────────────────────

/// The enum's variants in declaration (= discriminant) order.
pub fn enum_variants<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    enum_ref: EnumRef<'db>,
) -> Vec<Name> {
    match enum_ref {
        DeclRef::Source(enum_loc) => baml_compiler2_hir::item_data::enum_data(db, enum_loc)
            .variants
            .iter()
            .map(|variant| variant.name.clone())
            .collect(),
        DeclRef::External(enum_loc) => extern_enum_row(db, enum_loc).variants.to_vec(),
    }
}

/// The discriminant of the variant named `variant`, if the enum declares one.
pub fn enum_variant_index<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    enum_ref: EnumRef<'db>,
    variant: &Name,
) -> Option<u32> {
    let index = match enum_ref {
        DeclRef::Source(enum_loc) => baml_compiler2_hir::item_data::enum_data(db, enum_loc)
            .variants
            .iter()
            .position(|declared| declared.name == *variant)?,
        DeclRef::External(enum_loc) => extern_enum_row(db, enum_loc)
            .variants
            .iter()
            .position(|declared| declared == variant)?,
    };
    Some(u32::try_from(index).expect("enum variant count fits u32"))
}

// ── Interface layout ─────────────────────────────────────────────────────────

/// What an interface's OWN declaration says a member is.
///
/// Methods are checked before fields: a declaration carrying both names is
/// rejected upstream, and preferring the dispatchable reading keeps a
/// malformed interface from silently losing dispatch. The field index is the
/// position in the interface's own declared field list — the index space
/// every implementation's field links and every `VirtualFieldAccess` are
/// baked against, the same one `method_resolution::member_on_interface`
/// records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceMember {
    Method,
    Field { index: u32 },
}

/// What `interface`'s own declaration (not its `requires` closure) says
/// `member` is; `None` when it declares no such member.
pub fn interface_member<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    interface: InterfaceRef<'db>,
    member: &Name,
) -> Option<InterfaceMember> {
    let field_index = |position: usize| InterfaceMember::Field {
        index: u32::try_from(position).expect("interface field count fits u32"),
    };
    match interface {
        DeclRef::Source(interface) => {
            let data = baml_compiler2_hir::item_data::interface_data(db, interface);
            if data.methods.iter().any(|&method| {
                baml_compiler2_hir::item_data::function_data(db, method).name == *member
            }) {
                return Some(InterfaceMember::Method);
            }
            data.fields
                .iter()
                .position(|field| field.name == *member)
                .map(field_index)
        }
        DeclRef::External(interface) => {
            let row = extern_interface_row(db, interface);
            if row
                .required_methods
                .iter()
                .chain(row.default_methods)
                .any(|method| method.name == *member)
            {
                return Some(InterfaceMember::Method);
            }
            row.fields
                .iter()
                .position(|(name, ..)| name == member)
                .map(field_index)
        }
    }
}

/// Every method name `interface`'s own declaration carries, required and
/// default alike.
pub fn interface_method_names<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    interface: InterfaceRef<'db>,
) -> Vec<Name> {
    match interface {
        DeclRef::Source(interface) => baml_compiler2_hir::item_data::interface_data(db, interface)
            .methods
            .iter()
            .map(|&method| {
                baml_compiler2_hir::item_data::function_data(db, method)
                    .name
                    .clone()
            })
            .collect(),
        DeclRef::External(interface) => {
            let row = extern_interface_row(db, interface);
            row.required_methods
                .iter()
                .chain(row.default_methods)
                .map(|method| method.name.clone())
                .collect()
        }
    }
}

/// How many generic parameters `interface` declares (its frame minus `Self`).
pub fn interface_generic_arity<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    interface: InterfaceRef<'db>,
) -> usize {
    match interface {
        DeclRef::Source(interface) => baml_compiler2_hir::item_data::interface_data(db, interface)
            .generic_params
            .len(),
        DeclRef::External(interface) => extern_interface_row(db, interface).generic_params.len(),
    }
}

/// The interfaces `interface` DIRECTLY requires, by head. A source
/// declaration's targets are lowered as constraint heads; an exported row
/// carries its transitive closure, of which the direct targets are a
/// subset — the closure walk below is what makes the two agree.
fn interface_requires_heads<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    interface: InterfaceRef<'db>,
) -> Vec<DeclName> {
    match interface {
        DeclRef::Source(interface) => {
            let data = baml_compiler2_hir::item_data::interface_data(db, interface);
            let package = baml_compiler2_hir::file_package::file_package(db, interface.file(db));
            let items = baml_compiler2_hir::package::package_items(db, package.root);
            data.requires
                .iter()
                .filter_map(|&target| {
                    crate::interfaces::resolve_ref_to_interface_name(
                        db,
                        &data.type_refs,
                        target,
                        items,
                        &package.namespace_path,
                    )
                })
                .collect()
        }
        DeclRef::External(interface) => extern_interface_row(db, interface)
            .requires
            .iter()
            .map(|required: &Interface| required.name.clone())
            .collect(),
    }
}

/// The transitive `requires` closure of `interface`, the root first, in BFS
/// order, crossing lanes (a source interface may require a served one and
/// a served row may require an interface this database declares). A
/// required interface no lane declares is skipped — its absence is reported
/// where the declaration is checked, not here. Cycles are skipped silently
/// (E0118 reports them).
pub fn interface_requires_closure<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    interface: InterfaceRef<'db>,
) -> Vec<InterfaceRef<'db>> {
    let mut out = Vec::new();
    let mut queue = std::collections::VecDeque::from([interface]);
    while let Some(current) = queue.pop_front() {
        if out.contains(&current) {
            continue;
        }
        out.push(current);
        for head in interface_requires_heads(db, current) {
            if let Some(parent) = interface_ref_of(db, &head) {
                queue.push_back(parent);
            }
        }
    }
    out
}

// ── Alias value ──────────────────────────────────────────────────────────────

/// The type `alias` stands for, resolved one level (an alias body may name
/// other aliases; the alias ENVIRONMENT — `interfaces::package_resolved_aliases`
/// — closes over them).
pub fn alias_value<'db>(db: &'db dyn baml_compiler2_hir::Db, alias: AliasRef<'db>) -> Ty {
    match alias {
        DeclRef::Source(alias) => crate::lower::type_alias_value(db, alias),
        DeclRef::External(alias) => extern_alias_row(db, alias).clone(),
    }
}
