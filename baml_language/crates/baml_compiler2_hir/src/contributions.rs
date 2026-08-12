//! Symbol contributions — what names a file adds to its package namespace.

use baml_base::{Name, SourceFile};
use text_size::TextRange;

use crate::loc::{
    ClassLoc, ClientLoc, EnumLoc, FunctionLoc, InterfaceLoc, LetLoc, RetryPolicyLoc,
    TemplateStringLoc, TestLoc, TypeAliasLoc,
};

// ── DefinitionKind ──────────────────────────────────────────────────────────

/// Unified kind tag for any named definition, at any scope level.
///
/// Covers both namespace-level items (Class, Function, …) and intra-item
/// members (Field, Method, Variant, Binding, Parameter). Used by
/// `Definition<'db>::kind()` and `Hir2Diagnostic::DuplicateDefinition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefinitionKind {
    // Namespace-level items
    Class,
    Enum,
    Interface,
    TypeAlias,
    Function,
    TemplateString,
    Client,
    Test,
    RetryPolicy,
    Let,

    // Intra-item members
    Field,
    AssociatedType,
    Method,
    Variant,
    Binding,
    Parameter,
}

impl DefinitionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::TypeAlias => "type",
            Self::Function => "function",
            Self::TemplateString => "template_string",
            Self::Client => "client",
            Self::Test => "test",
            Self::RetryPolicy => "retry_policy",
            Self::Let => "let",
            Self::Field => "field",
            Self::AssociatedType => "associated type",
            Self::Method => "method",
            Self::Variant => "variant",
            Self::Binding => "binding",
            Self::Parameter => "parameter",
        }
    }

    /// Whether this kind uses dot-qualified names (e.g. `Foo.bar`).
    /// Members of a type use dot notation; locals use "in" phrasing.
    pub fn is_member(self) -> bool {
        matches!(
            self,
            Self::Field | Self::AssociatedType | Self::Method | Self::Variant
        )
    }
}

impl std::fmt::Display for DefinitionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Definition ──────────────────────────────────────────────────────────────

/// What a single definition resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Definition<'db> {
    Class(ClassLoc<'db>),
    Enum(EnumLoc<'db>),
    Interface(InterfaceLoc<'db>),
    TypeAlias(TypeAliasLoc<'db>),
    Function(FunctionLoc<'db>),
    TemplateString(TemplateStringLoc<'db>),
    Client(ClientLoc<'db>),
    Test(TestLoc<'db>),
    RetryPolicy(RetryPolicyLoc<'db>),
    Let(LetLoc<'db>),
}

impl<'db> Definition<'db> {
    /// The source file this definition lives in.
    pub fn file(self, db: &'db dyn crate::Db) -> SourceFile {
        match self {
            Definition::Class(loc) => loc.file(db),
            Definition::Enum(loc) => loc.file(db),
            Definition::Interface(loc) => loc.file(db),
            Definition::TypeAlias(loc) => loc.file(db),
            Definition::Function(loc) => loc.file(db),
            Definition::TemplateString(loc) => loc.file(db),
            Definition::Client(loc) => loc.file(db),
            Definition::Test(loc) => loc.file(db),
            Definition::RetryPolicy(loc) => loc.file(db),
            Definition::Let(loc) => loc.file(db),
        }
    }

    /// The unified kind tag for this definition.
    pub fn kind(self) -> DefinitionKind {
        match self {
            Definition::Class(_) => DefinitionKind::Class,
            Definition::Enum(_) => DefinitionKind::Enum,
            Definition::Interface(_) => DefinitionKind::Interface,
            Definition::TypeAlias(_) => DefinitionKind::TypeAlias,
            Definition::Function(_) => DefinitionKind::Function,
            Definition::TemplateString(_) => DefinitionKind::TemplateString,
            Definition::Client(_) => DefinitionKind::Client,
            Definition::Test(_) => DefinitionKind::Test,
            Definition::RetryPolicy(_) => DefinitionKind::RetryPolicy,
            Definition::Let(_) => DefinitionKind::Let,
        }
    }

    /// Human-readable kind label for diagnostics.
    pub fn kind_name(self) -> &'static str {
        self.kind().as_str()
    }

    /// The declaration kind as written in BAML source.
    ///
    /// Some configuration declarations are lowered to top-level lets before
    /// HIR. Keep that internal representation from leaking into source-level
    /// namespace diagnostics.
    pub fn source_kind(self, db: &'db dyn crate::Db) -> DefinitionKind {
        let Definition::Let(loc) = self else {
            return self.kind();
        };
        let item_tree = crate::file_item_tree(db, loc.file(db));
        match item_tree.lets.get(&loc.id(db)).map(|item| item.origin) {
            Some(baml_compiler2_ast::LetOrigin::Client) => DefinitionKind::Client,
            Some(baml_compiler2_ast::LetOrigin::RetryPolicy) => DefinitionKind::RetryPolicy,
            Some(baml_compiler2_ast::LetOrigin::Source) | None => DefinitionKind::Let,
        }
    }

    pub fn source_kind_name(self, db: &'db dyn crate::Db) -> &'static str {
        self.source_kind(db).as_str()
    }

    /// Whether this definition is outside BAML's user-facing language surface.
    pub fn is_language_internal(self, db: &'db dyn crate::Db) -> bool {
        let Definition::Function(loc) = self else {
            return false;
        };
        crate::file_item_tree(db, loc.file(db))[loc.id(db)]
            .metadata
            .is_language_internal
    }
}

/// A symbol contribution: name, definition, and the name's source span.
///
/// The `TextRange` is the span of the name token (not the full item),
/// used by namespace conflict diagnostics to point at the exact name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contribution<'db> {
    pub name_span: TextRange,
    pub definition: Definition<'db>,
}

/// Names contributed by a single file to its package namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSymbolContributions<'db> {
    /// Type-namespace names (classes, enums, type aliases).
    pub types: Vec<(Name, Contribution<'db>)>,
    /// Value-namespace names (functions, template strings, clients, etc.).
    pub values: Vec<(Name, Contribution<'db>)>,
}

impl FileSymbolContributions<'_> {
    pub fn new() -> Self {
        Self {
            types: Vec::new(),
            values: Vec::new(),
        }
    }
}

impl Default for FileSymbolContributions<'_> {
    fn default() -> Self {
        Self::new()
    }
}
