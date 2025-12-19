//! Symbol lookup and resolution for LSP operations.
//!
//! This module provides APIs for finding symbols by name, finding symbols at positions,
//! and resolving symbol references.

use std::path::Path;

use baml_db::{
    Name, SourceFile, Span,
    baml_hir::{
        self, ClassId, ClientId, EnumId, FunctionId, ItemId, TestId, TypeAliasId, file_item_tree,
        file_items, project_items,
    },
};
use lsp_types::{Location, Position, Range, Url};
use text_size::TextRange;

use super::{
    LspDatabase,
    position::{LineIndex, span_to_lsp_range},
};

/// The kind of a symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Class,
    Enum,
    TypeAlias,
    Client,
    Test,
    /// A field within a class.
    Field,
    /// A variant within an enum.
    EnumVariant,
}

impl SymbolKind {
    /// Convert to LSP symbol kind.
    pub fn to_lsp_symbol_kind(&self) -> lsp_types::SymbolKind {
        match self {
            SymbolKind::Function => lsp_types::SymbolKind::FUNCTION,
            SymbolKind::Class => lsp_types::SymbolKind::CLASS,
            SymbolKind::Enum => lsp_types::SymbolKind::ENUM,
            SymbolKind::TypeAlias => lsp_types::SymbolKind::TYPE_PARAMETER,
            SymbolKind::Client => lsp_types::SymbolKind::OBJECT,
            SymbolKind::Test => lsp_types::SymbolKind::METHOD,
            SymbolKind::Field => lsp_types::SymbolKind::FIELD,
            SymbolKind::EnumVariant => lsp_types::SymbolKind::ENUM_MEMBER,
        }
    }
}

/// Information about a found symbol.
#[derive(Clone)]
pub struct SymbolInfo {
    /// The name of the symbol.
    pub name: String,
    /// The kind of symbol.
    pub kind: SymbolKind,
    /// The file containing the symbol.
    pub file: SourceFile,
    /// The span of the symbol definition.
    pub span: Span,
}

/// A symbol location for LSP responses.
#[derive(Debug, Clone)]
pub struct SymbolLocation {
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: std::path::PathBuf,
    pub range: Range,
}

impl SymbolLocation {
    /// Convert to an LSP Location.
    pub fn to_lsp_location(&self) -> Option<Location> {
        let uri = Url::from_file_path(&self.file_path).ok()?;
        Some(Location {
            uri,
            range: self.range,
        })
    }
}

impl LspDatabase {
    /// Find a symbol by name across all files in the project.
    ///
    /// Returns the first matching symbol, or None if not found.
    pub fn find_symbol(&self, name: &str) -> Option<SymbolLocation> {
        self.find_symbol_locations(name).into_iter().next()
    }

    /// Find all locations where a symbol with the given name is defined.
    ///
    /// This supports multi-file definitions (e.g., partial classes spread across files).
    pub fn find_symbol_locations(&self, name: &str) -> Vec<SymbolLocation> {
        let mut locations = Vec::new();
        let name_to_find = Name::new(name);

        // If we have a project, search all project items
        if let Some(project) = self.project {
            let items = project_items(&self.db, project);

            for item in items.items(&self.db) {
                if let Some(loc) = self.item_to_location(*item, &name_to_find) {
                    locations.push(loc);
                }
            }
        } else {
            // No project - search all files individually
            for file in self.files() {
                let items = file_items(&self.db, file);
                for item in items.items(&self.db) {
                    if let Some(loc) = self.item_to_location(*item, &name_to_find) {
                        locations.push(loc);
                    }
                }
            }
        }

        locations
    }

    /// Convert an ItemId to a SymbolLocation if it matches the given name.
    fn item_to_location<'db>(
        &'db self,
        item: ItemId<'db>,
        name_to_find: &Name,
    ) -> Option<SymbolLocation> {
        match item {
            ItemId::Function(func_loc) => {
                let file = func_loc.file(&self.db);
                let item_tree = file_item_tree(&self.db, file);
                let func = &item_tree[func_loc.id(&self.db)];

                if &func.name == name_to_find {
                    let file_path = file.path(&self.db);
                    let text = file.text(&self.db);

                    // For now, create a span at the start of the file
                    // TODO: Store actual span in ItemTree
                    let span = Span::new(file.file_id(&self.db), TextRange::empty(0.into()));
                    let range = span_to_lsp_range(text, &span);

                    Some(SymbolLocation {
                        name: func.name.to_string(),
                        kind: SymbolKind::Function,
                        file_path,
                        range,
                    })
                } else {
                    None
                }
            }
            ItemId::Class(class_loc) => {
                let file = class_loc.file(&self.db);
                let item_tree = file_item_tree(&self.db, file);
                let class = &item_tree[class_loc.id(&self.db)];

                if &class.name == name_to_find {
                    let file_path = file.path(&self.db);
                    let text = file.text(&self.db);
                    let span = Span::new(file.file_id(&self.db), TextRange::empty(0.into()));
                    let range = span_to_lsp_range(text, &span);

                    Some(SymbolLocation {
                        name: class.name.to_string(),
                        kind: SymbolKind::Class,
                        file_path,
                        range,
                    })
                } else {
                    // Also check class fields
                    for field in &class.fields {
                        if &field.name == name_to_find {
                            let file_path = file.path(&self.db);
                            let text = file.text(&self.db);
                            let span =
                                Span::new(file.file_id(&self.db), TextRange::empty(0.into()));
                            let range = span_to_lsp_range(text, &span);

                            return Some(SymbolLocation {
                                name: field.name.to_string(),
                                kind: SymbolKind::Field,
                                file_path,
                                range,
                            });
                        }
                    }
                    None
                }
            }
            ItemId::Enum(enum_loc) => {
                let file = enum_loc.file(&self.db);
                let item_tree = file_item_tree(&self.db, file);
                let enum_def = &item_tree[enum_loc.id(&self.db)];

                if &enum_def.name == name_to_find {
                    let file_path = file.path(&self.db);
                    let text = file.text(&self.db);
                    let span = Span::new(file.file_id(&self.db), TextRange::empty(0.into()));
                    let range = span_to_lsp_range(text, &span);

                    Some(SymbolLocation {
                        name: enum_def.name.to_string(),
                        kind: SymbolKind::Enum,
                        file_path,
                        range,
                    })
                } else {
                    // Also check enum variants
                    for variant in &enum_def.variants {
                        if &variant.name == name_to_find {
                            let file_path = file.path(&self.db);
                            let text = file.text(&self.db);
                            let span =
                                Span::new(file.file_id(&self.db), TextRange::empty(0.into()));
                            let range = span_to_lsp_range(text, &span);

                            return Some(SymbolLocation {
                                name: variant.name.to_string(),
                                kind: SymbolKind::EnumVariant,
                                file_path,
                                range,
                            });
                        }
                    }
                    None
                }
            }
            ItemId::TypeAlias(alias_loc) => {
                let file = alias_loc.file(&self.db);
                let item_tree = file_item_tree(&self.db, file);
                let alias = &item_tree[alias_loc.id(&self.db)];

                if &alias.name == name_to_find {
                    let file_path = file.path(&self.db);
                    let text = file.text(&self.db);
                    let span = Span::new(file.file_id(&self.db), TextRange::empty(0.into()));
                    let range = span_to_lsp_range(text, &span);

                    Some(SymbolLocation {
                        name: alias.name.to_string(),
                        kind: SymbolKind::TypeAlias,
                        file_path,
                        range,
                    })
                } else {
                    None
                }
            }
            ItemId::Client(client_loc) => {
                let file = client_loc.file(&self.db);
                let item_tree = file_item_tree(&self.db, file);
                let client = &item_tree[client_loc.id(&self.db)];

                if &client.name == name_to_find {
                    let file_path = file.path(&self.db);
                    let text = file.text(&self.db);
                    let span = Span::new(file.file_id(&self.db), TextRange::empty(0.into()));
                    let range = span_to_lsp_range(text, &span);

                    Some(SymbolLocation {
                        name: client.name.to_string(),
                        kind: SymbolKind::Client,
                        file_path,
                        range,
                    })
                } else {
                    None
                }
            }
            ItemId::Test(test_loc) => {
                let file = test_loc.file(&self.db);
                let item_tree = file_item_tree(&self.db, file);
                let test = &item_tree[test_loc.id(&self.db)];

                if &test.name == name_to_find {
                    let file_path = file.path(&self.db);
                    let text = file.text(&self.db);
                    let span = Span::new(file.file_id(&self.db), TextRange::empty(0.into()));
                    let range = span_to_lsp_range(text, &span);

                    Some(SymbolLocation {
                        name: test.name.to_string(),
                        kind: SymbolKind::Test,
                        file_path,
                        range,
                    })
                } else {
                    None
                }
            }
        }
    }

    /// Get information about the symbol at a specific position in a file.
    ///
    /// This is used for hover and go-to-definition operations.
    pub fn symbol_at_position(&self, file: SourceFile, pos: &Position) -> Option<SymbolInfo> {
        let text = file.text(&self.db);

        // Get the word at the cursor position
        let (word, _range) = super::position::get_word_at_position(text, pos)?;

        // Look up the symbol by name
        let symbol_loc = self.find_symbol(&word)?;

        // Get the file containing the definition
        let def_file = self.get_file(&symbol_loc.file_path)?;

        Some(SymbolInfo {
            name: symbol_loc.name,
            kind: symbol_loc.kind,
            file: def_file,
            span: Span::new(
                def_file.file_id(&self.db),
                TextRange::empty(0.into()), // TODO: actual span
            ),
        })
    }

    /// List all functions in the project.
    pub fn list_functions(&self) -> Vec<SymbolLocation> {
        let mut functions = Vec::new();

        if let Some(project) = self.project {
            let items = project_items(&self.db, project);
            for item in items.items(&self.db) {
                if let ItemId::Function(func_loc) = item {
                    let file = func_loc.file(&self.db);
                    let item_tree = file_item_tree(&self.db, file);
                    let func = &item_tree[func_loc.id(&self.db)];

                    let file_path = file.path(&self.db);
                    let text = file.text(&self.db);
                    let span = Span::new(file.file_id(&self.db), TextRange::empty(0.into()));
                    let range = span_to_lsp_range(text, &span);

                    functions.push(SymbolLocation {
                        name: func.name.to_string(),
                        kind: SymbolKind::Function,
                        file_path,
                        range,
                    });
                }
            }
        }

        functions
    }

    /// List all classes in the project.
    pub fn list_classes(&self) -> Vec<SymbolLocation> {
        let mut classes = Vec::new();

        if let Some(project) = self.project {
            let items = project_items(&self.db, project);
            for item in items.items(&self.db) {
                if let ItemId::Class(class_loc) = item {
                    let file = class_loc.file(&self.db);
                    let item_tree = file_item_tree(&self.db, file);
                    let class = &item_tree[class_loc.id(&self.db)];

                    let file_path = file.path(&self.db);
                    let text = file.text(&self.db);
                    let span = Span::new(file.file_id(&self.db), TextRange::empty(0.into()));
                    let range = span_to_lsp_range(text, &span);

                    classes.push(SymbolLocation {
                        name: class.name.to_string(),
                        kind: SymbolKind::Class,
                        file_path,
                        range,
                    });
                }
            }
        }

        classes
    }

    /// List all enums in the project.
    pub fn list_enums(&self) -> Vec<SymbolLocation> {
        let mut enums = Vec::new();

        if let Some(project) = self.project {
            let items = project_items(&self.db, project);
            for item in items.items(&self.db) {
                if let ItemId::Enum(enum_loc) = item {
                    let file = enum_loc.file(&self.db);
                    let item_tree = file_item_tree(&self.db, file);
                    let enum_def = &item_tree[enum_loc.id(&self.db)];

                    let file_path = file.path(&self.db);
                    let text = file.text(&self.db);
                    let span = Span::new(file.file_id(&self.db), TextRange::empty(0.into()));
                    let range = span_to_lsp_range(text, &span);

                    enums.push(SymbolLocation {
                        name: enum_def.name.to_string(),
                        kind: SymbolKind::Enum,
                        file_path,
                        range,
                    });
                }
            }
        }

        enums
    }

    /// Get the hover text for a symbol.
    ///
    /// Returns formatted documentation/signature for the symbol.
    pub fn get_hover_text(&self, symbol: &SymbolInfo) -> String {
        match symbol.kind {
            SymbolKind::Function => {
                // Try to get function signature
                if let Some(project) = self.project {
                    let items = project_items(&self.db, project);
                    for item in items.items(&self.db) {
                        if let ItemId::Function(func_loc) = item {
                            let file = func_loc.file(&self.db);
                            let item_tree = file_item_tree(&self.db, file);
                            let func = &item_tree[func_loc.id(&self.db)];

                            if func.name.as_str() == symbol.name {
                                let sig = baml_hir::function_signature(&self.db, *func_loc);
                                return format_function_signature(&sig);
                            }
                        }
                    }
                }
                format!("function {}", symbol.name)
            }
            SymbolKind::Class => {
                // Get class fields for hover
                if let Some(project) = self.project {
                    let items = project_items(&self.db, project);
                    for item in items.items(&self.db) {
                        if let ItemId::Class(class_loc) = item {
                            let file = class_loc.file(&self.db);
                            let item_tree = file_item_tree(&self.db, file);
                            let class = &item_tree[class_loc.id(&self.db)];

                            if class.name.as_str() == symbol.name {
                                return format_class_definition(class);
                            }
                        }
                    }
                }
                format!("class {}", symbol.name)
            }
            SymbolKind::Enum => {
                if let Some(project) = self.project {
                    let items = project_items(&self.db, project);
                    for item in items.items(&self.db) {
                        if let ItemId::Enum(enum_loc) = item {
                            let file = enum_loc.file(&self.db);
                            let item_tree = file_item_tree(&self.db, file);
                            let enum_def = &item_tree[enum_loc.id(&self.db)];

                            if enum_def.name.as_str() == symbol.name {
                                return format_enum_definition(enum_def);
                            }
                        }
                    }
                }
                format!("enum {}", symbol.name)
            }
            SymbolKind::TypeAlias => format!("type {}", symbol.name),
            SymbolKind::Client => format!("client {}", symbol.name),
            SymbolKind::Test => format!("test {}", symbol.name),
            SymbolKind::Field => format!("field {}", symbol.name),
            SymbolKind::EnumVariant => format!("variant {}", symbol.name),
        }
    }
}

/// Format a function signature for hover display.
fn format_function_signature(sig: &baml_hir::FunctionSignature) -> String {
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, format_type_ref(&p.type_ref)))
        .collect();

    format!(
        "function {}({}) -> {}",
        sig.name,
        params.join(", "),
        format_type_ref(&sig.return_type)
    )
}

/// Format a TypeRef for display.
fn format_type_ref(ty: &baml_hir::TypeRef) -> String {
    use baml_db::baml_hir::TypeRef;

    match ty {
        TypeRef::Path(path) => path
            .segments
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("::"),
        TypeRef::Int => "int".to_string(),
        TypeRef::Float => "float".to_string(),
        TypeRef::String => "string".to_string(),
        TypeRef::Bool => "bool".to_string(),
        TypeRef::Null => "null".to_string(),
        TypeRef::Image => "image".to_string(),
        TypeRef::Audio => "audio".to_string(),
        TypeRef::Video => "video".to_string(),
        TypeRef::Pdf => "pdf".to_string(),
        TypeRef::Optional(inner) => format!("{}?", format_type_ref(inner)),
        TypeRef::List(inner) => format!("{}[]", format_type_ref(inner)),
        TypeRef::Map { key, value } => {
            format!("map<{}, {}>", format_type_ref(key), format_type_ref(value))
        }
        TypeRef::Union(types) => types
            .iter()
            .map(format_type_ref)
            .collect::<Vec<_>>()
            .join(" | "),
        TypeRef::StringLiteral(s) => format!("\"{}\"", s),
        TypeRef::IntLiteral(i) => i.to_string(),
        TypeRef::FloatLiteral(f) => f.clone(),
        TypeRef::Generic { base, args } => {
            let args_str = args
                .iter()
                .map(format_type_ref)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", format_type_ref(base), args_str)
        }
        TypeRef::TypeParam(name) => name.to_string(),
        TypeRef::Error => "<error>".to_string(),
        TypeRef::Unknown => "<unknown>".to_string(),
    }
}

/// Format a class definition for hover display.
fn format_class_definition(class: &baml_hir::Class) -> String {
    let mut lines = vec![format!("class {} {{", class.name)];

    for field in &class.fields {
        lines.push(format!(
            "  {} {}",
            field.name,
            format_type_ref(&field.type_ref)
        ));
    }

    lines.push("}".to_string());

    if class.is_dynamic {
        lines.push("// @@dynamic".to_string());
    }

    lines.join("\n")
}

/// Format an enum definition for hover display.
fn format_enum_definition(enum_def: &baml_hir::Enum) -> String {
    let mut lines = vec![format!("enum {} {{", enum_def.name)];

    for variant in &enum_def.variants {
        lines.push(format!("  {}", variant.name));
    }

    lines.push("}".to_string());
    lines.join("\n")
}
