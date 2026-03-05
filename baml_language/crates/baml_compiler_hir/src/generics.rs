//! Generic parameters for functions and types.
//!
//! Following rust-analyzer's pattern, generic parameters are queried separately
//! from the `ItemTree` to maintain the invalidation barrier. Changes to generic
//! parameters don't invalidate the `ItemTree`.

use std::sync::Arc;

use baml_base::{Name, SourceFile};
use baml_compiler_parser::syntax_tree;
use baml_compiler_syntax::ast;
use la_arena::{Arena, Idx};
use rowan::ast::AstNode;

use crate::fqn::QualifiedName;
use crate::ids::{ItemKind, LocalIdAllocator};
use crate::{ClassId, Db, EnumId, FunctionId, TypeAliasId};

/// Type parameter in a generic definition.
///
/// Example: `T` in `class Foo<T>`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeParam {
    pub name: Name,
    // Future: bounds, defaults, constraints
}

/// Local index for a type parameter within its `GenericParams`.
pub type LocalTypeParamId = Idx<TypeParam>;

/// Generic parameters for an item (function, class, enum, etc.).
///
/// This is queried separately from the `ItemTree` for incrementality.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GenericParams {
    /// Type parameters arena.
    pub type_params: Arena<TypeParam>,
    // Future: const parameters, lifetime parameters, where clauses
}

impl GenericParams {
    /// Create empty generic parameters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if there are any generic parameters.
    pub fn is_empty(&self) -> bool {
        self.type_params.is_empty()
    }

    /// Get all type parameter names.
    pub fn type_param_names(&self) -> impl Iterator<Item = &Name> {
        self.type_params.iter().map(|(_, p)| &p.name)
    }
}

impl std::ops::Index<LocalTypeParamId> for GenericParams {
    type Output = TypeParam;

    fn index(&self, index: LocalTypeParamId) -> &Self::Output {
        &self.type_params[index]
    }
}

fn empty_generic_params() -> Arc<GenericParams> {
    Arc::new(GenericParams::new())
}

fn generic_params_from_list(list: Option<ast::GenericParamList>) -> Arc<GenericParams> {
    let Some(list) = list else {
        return empty_generic_params();
    };
    let mut params = GenericParams::new();
    for param_token in list.params() {
        params.type_params.alloc(TypeParam {
            name: Name::new(param_token.text()),
        });
    }
    Arc::new(params)
}

fn with_source_file<T>(
    db: &dyn Db,
    file: SourceFile,
    f: impl FnOnce(ast::SourceFile) -> T,
) -> Option<T> {
    let tree = syntax_tree(db, file);
    ast::SourceFile::cast(tree).map(f)
}

fn find_top_level_generic_params(
    source_file: &ast::SourceFile,
    allocator: &mut LocalIdAllocator,
    target_id: u32,
    item_kind: ItemKind,
    mut extract: impl FnMut(ast::Item) -> Option<(Name, Option<ast::GenericParamList>)>,
) -> Option<Arc<GenericParams>> {
    for item in source_file.items() {
        if let Some((name, list)) = extract(item) {
            let id = allocator.alloc_id::<()>(item_kind, &name);
            if id.as_u32() == target_id {
                return Some(generic_params_from_list(list));
            }
        }
    }
    None
}

fn scan_function_item_for_generics(
    allocator: &mut LocalIdAllocator,
    func_node: &ast::FunctionDef,
    target_id: u32,
) -> Option<Arc<GenericParams>> {
    let name_token = func_node.name()?;
    let base_name = Name::new(name_token.text());
    let base_id = allocator.alloc_id::<()>(ItemKind::Function, &base_name);
    if base_id.as_u32() == target_id {
        return Some(generic_params_from_list(func_node.generic_param_list()));
    }

    func_node.llm_body()?;

    let render_name = Name::new(format!("{base_name}.render_prompt"));
    let render_id = allocator.alloc_id::<()>(ItemKind::Function, &render_name);
    if render_id.as_u32() == target_id {
        return Some(generic_params_from_list(func_node.generic_param_list()));
    }

    let build_name = Name::new(format!("{base_name}.build_request"));
    let build_id = allocator.alloc_id::<()>(ItemKind::Function, &build_name);
    if build_id.as_u32() == target_id {
        return Some(generic_params_from_list(func_node.generic_param_list()));
    }

    None
}

fn scan_class_methods_for_generics(
    allocator: &mut LocalIdAllocator,
    class_node: &ast::ClassDef,
    target_id: u32,
) -> Option<Arc<GenericParams>> {
    let class_name = class_node
        .name()
        .map(|token| token.text().to_string())
        .unwrap_or_else(|| "UnnamedClass".to_string());
    for method in class_node.methods() {
        let Some(method_name) = method.name() else {
            continue;
        };
        let qualified_method_name =
            QualifiedName::local_method_from_str(&class_name, method_name.text());
        let id = allocator.alloc_id::<()>(ItemKind::Function, &qualified_method_name);
        if id.as_u32() == target_id {
            return Some(generic_params_from_list(method.generic_param_list()));
        }
    }
    None
}

pub(crate) fn function_generic_params_from_cst(
    db: &dyn Db,
    func: FunctionId<'_>,
) -> Arc<GenericParams> {
    let file = func.file(db);
    let target_id = func.id(db).as_u32();

    let result = with_source_file(db, file, |source_file| {
        let mut allocator = LocalIdAllocator::new();
        for item in source_file.items() {
            match item {
                ast::Item::Function(func_node) => {
                    if let Some(params) =
                        scan_function_item_for_generics(&mut allocator, &func_node, target_id)
                    {
                        return params;
                    }
                }
                ast::Item::Class(class_node) => {
                    if let Some(params) =
                        scan_class_methods_for_generics(&mut allocator, &class_node, target_id)
                    {
                        return params;
                    }
                }
                ast::Item::Client(client_node) => {
                    if let Some(name_token) = client_node.name() {
                        let resolve_name = Name::new(format!("{}.resolve", name_token.text()));
                        let resolve_id =
                            allocator.alloc_id::<()>(ItemKind::Function, &resolve_name);
                        if resolve_id.as_u32() == target_id {
                            return empty_generic_params();
                        }
                    }
                }
                _ => {}
            }
        }
        empty_generic_params()
    });

    result.unwrap_or_else(empty_generic_params)
}

pub(crate) fn class_generic_params_from_cst(db: &dyn Db, class: ClassId<'_>) -> Arc<GenericParams> {
    let file = class.file(db);
    let target_id = class.id(db).as_u32();

    let result = with_source_file(db, file, |source_file| {
        let mut allocator = LocalIdAllocator::new();
        find_top_level_generic_params(
            &source_file,
            &mut allocator,
            target_id,
            ItemKind::Class,
            |item| {
                if let ast::Item::Class(class_node) = item {
                    class_node.name().map(|name_token| {
                        (
                            Name::new(name_token.text()),
                            class_node.generic_param_list(),
                        )
                    })
                } else {
                    None
                }
            },
        )
    });

    result.flatten().unwrap_or_else(empty_generic_params)
}

pub(crate) fn enum_generic_params_from_cst(
    db: &dyn Db,
    enum_def: EnumId<'_>,
) -> Arc<GenericParams> {
    let file = enum_def.file(db);
    let target_id = enum_def.id(db).as_u32();

    let result = with_source_file(db, file, |source_file| {
        let mut allocator = LocalIdAllocator::new();
        find_top_level_generic_params(
            &source_file,
            &mut allocator,
            target_id,
            ItemKind::Enum,
            |item| {
                if let ast::Item::Enum(enum_node) = item {
                    enum_node.name().map(|name_token| {
                        (Name::new(name_token.text()), enum_node.generic_param_list())
                    })
                } else {
                    None
                }
            },
        )
    });

    result.flatten().unwrap_or_else(empty_generic_params)
}

pub(crate) fn type_alias_generic_params_from_cst(
    db: &dyn Db,
    alias: TypeAliasId<'_>,
) -> Arc<GenericParams> {
    let file = alias.file(db);
    let target_id = alias.id(db).as_u32();

    let result = with_source_file(db, file, |source_file| {
        let mut allocator = LocalIdAllocator::new();
        find_top_level_generic_params(
            &source_file,
            &mut allocator,
            target_id,
            ItemKind::TypeAlias,
            |item| {
                if let ast::Item::TypeAlias(alias_node) = item {
                    alias_node.name().map(|name_token| {
                        (
                            Name::new(name_token.text()),
                            alias_node.generic_param_list(),
                        )
                    })
                } else {
                    None
                }
            },
        )
    });

    result.flatten().unwrap_or_else(empty_generic_params)
}
