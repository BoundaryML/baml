use baml_base::SourceRoot;
use baml_compiler2_emit::{function_path, owns_no_slot};
use baml_compiler2_hir::{
    contributions::Definition,
    file_package::file_package,
    item_data::{class_data, enum_data, let_data},
    loc::{DeclRef, FunctionLoc},
};
use baml_compiler2_hir_ty::{
    callable::ExternalCallTarget,
    extern_loc::{ClassRef, EnumRef, ExternFunctionLoc, ExternRowAddr, FunctionRef, GlobalRef},
    lower::qualify_def,
};
use baml_type::DeclName;
use bex_vm_types::{BodyKey, DeclPath, FnPath, types::LocalName};

pub(crate) fn item_path(head: &DeclName) -> LocalName {
    LocalName {
        namespace: head.namespace().clone(),
        name: head.name().clone(),
    }
}

fn class_head<'db>(db: &'db dyn baml_compiler2_mir::Db, class: ClassRef<'db>) -> DeclName {
    match class {
        DeclRef::Source(class) => {
            qualify_def(db, Definition::Class(class), &class_data(db, class).name)
        }
        DeclRef::External(class) => class.head(db).clone(),
    }
}

fn enum_head<'db>(db: &'db dyn baml_compiler2_mir::Db, enum_ref: EnumRef<'db>) -> DeclName {
    match enum_ref {
        DeclRef::Source(enum_loc) => qualify_def(
            db,
            Definition::Enum(enum_loc),
            &enum_data(db, enum_loc).name,
        ),
        DeclRef::External(enum_loc) => enum_loc.head(db).clone(),
    }
}

fn extern_function_coords<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    row: ExternFunctionLoc<'db>,
) -> Option<DeclCoords> {
    Some(match row.addr(db) {
        ExternRowAddr::Declared(target) => match target {
            ExternalCallTarget::Free { function } => (
                function.root(),
                DeclPath::Function(FnPath::Free(item_path(function))),
            ),
            ExternalCallTarget::Method { class, name } => (
                class.root(),
                DeclPath::Function(FnPath::Method {
                    class: item_path(class),
                    name: name.clone(),
                }),
            ),
            ExternalCallTarget::Interface { interface, method } => (
                interface.root(),
                DeclPath::InterfaceBody(BodyKey::Default {
                    interface: item_path(interface),
                    method: method.clone(),
                }),
            ),
        },
        ExternRowAddr::ImplProvided { .. } => return None,
    })
}

/// Package root and declaration path: the identity used by linked package tables.
pub(crate) type DeclCoords = (SourceRoot, DeclPath);

/// The coordinates of a callable, wherever it is declared. `None` for a
/// function that owns no cell — a required interface method, a builtin that
/// lowers at its call site — and for an impl-provided row, a body no call
/// names directly.
pub(crate) fn function_coords<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    function: FunctionRef<'db>,
) -> Option<DeclCoords> {
    match function {
        DeclRef::Source(function) => declared_function_coords(db, function),
        DeclRef::External(row) => extern_function_coords(db, row),
    }
}

pub(crate) fn declared_function_coords<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    function: FunctionLoc<'db>,
) -> Option<DeclCoords> {
    if owns_no_slot(db, function) {
        return None;
    }
    let root = file_package(db, function.file(db)).root;
    Some((root, function_path(db, function)))
}

pub(crate) fn class_coords<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    class: ClassRef<'db>,
) -> DeclCoords {
    class_head_coords(&class_head(db, class))
}

pub(crate) fn class_head_coords(head: &DeclName) -> DeclCoords {
    (head.root(), DeclPath::Class(item_path(head)))
}

pub(crate) fn enum_head_coords(head: &DeclName) -> DeclCoords {
    (head.root(), DeclPath::Enum(item_path(head)))
}

pub(crate) fn interface_head_coords(head: &DeclName) -> DeclCoords {
    (head.root(), DeclPath::Interface(item_path(head)))
}

pub(crate) fn enum_coords<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    enum_ref: EnumRef<'db>,
) -> DeclCoords {
    enum_head_coords(&enum_head(db, enum_ref))
}

pub(crate) fn let_coords<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    binding: GlobalRef<'db>,
) -> DeclCoords {
    let head = match binding {
        DeclRef::Source(binding) => {
            qualify_def(db, Definition::Let(binding), &let_data(db, binding).name)
        }
        DeclRef::External(binding) => binding.head(db).clone(),
    };
    (head.root(), DeclPath::Let(item_path(&head)))
}
