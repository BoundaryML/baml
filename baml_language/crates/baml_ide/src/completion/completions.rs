//! The accumulator (rust-analyzer's `Completions`): providers say WHAT is
//! offered, and this module is the one place that decides HOW an offer is
//! presented — its insert text, its kind, and the relevance facts it ranks
//! by — and WHETHER it is offered at all, which is the same argument: a
//! visibility rule stated here holds for every provider at once, which is
//! exactly what the hand-built item literals this replaces could not
//! promise.

use baml_base::{Name, SourceFile};
use baml_compiler2_hir::{
    contributions::DefinitionKind,
    resolve::{
        NamespaceMember, NamespaceMemberKind, ScopeName, ScopeNameKind, TypeScopeName,
        TypeScopeNameKind,
    },
};
use baml_compiler2_hir_ty::method_resolution::{MemberCandidate, MemberDecl, MemberSource};
use text_size::TextRange;

use super::{
    item::{Completion, CompletionInsert, CompletionKind, CompletionRelevance},
    render,
    render::MemberForm,
};

pub(super) struct Completions<'db> {
    db: &'db dyn baml_compiler2_hir::Db,
    /// The file being edited: types render relative to it.
    file: SourceFile,
    /// The range every accepted item replaces: the fragment already typed.
    source_range: TextRange,
    /// Whether the stdlib's internals belong in this list, decided once
    /// from what the reader has typed.
    internals: crate::symbols::Internals,
    items: Vec<Completion>,
}

impl<'db> Completions<'db> {
    /// `typed` is the fragment the reader has already written at the cursor
    /// — exactly the text `source_range` covers.
    pub(super) fn new(
        db: &'db dyn baml_compiler2_hir::Db,
        file: SourceFile,
        source_range: TextRange,
        typed: &str,
    ) -> Self {
        Self {
            db,
            file,
            source_range,
            internals: crate::symbols::Internals::for_query(typed),
            items: Vec::new(),
        }
    }

    /// Best first, ties on the label so the list is stable between
    /// keystrokes.
    pub(super) fn into_sorted(mut self) -> Vec<Completion> {
        self.items.sort_by(|a, b| {
            b.relevance
                .score()
                .cmp(&a.relevance.score())
                .then_with(|| a.label.cmp(&b.label))
        });
        self.items
    }

    /// A member reached through a dot — of a value or of a type; `form`
    /// says which, and decides how its signature reads.
    pub(super) fn add_member(&mut self, candidate: &MemberCandidate<'_>, form: MemberForm) {
        use baml_compiler2_hir::loc::DeclRef;

        let (detail, documentation) = render::member(self.db, self.file, &candidate.decl, form);
        // A served package exports rows, not source: there is no file to
        // point at, and no served package is the stdlib's source.
        let declared_in = match candidate.decl {
            MemberDecl::Method(DeclRef::Source(function)) => Some(function.file(self.db)),
            MemberDecl::ClassField {
                class: DeclRef::Source(class),
                ..
            } => Some(class.file(self.db)),
            MemberDecl::InterfaceField {
                interface: DeclRef::Source(interface),
                ..
            } => Some(interface.file(self.db)),
            MemberDecl::EnumVariant {
                enum_loc: DeclRef::Source(enum_loc),
                ..
            } => Some(enum_loc.file(self.db)),
            MemberDecl::Method(DeclRef::External(_))
            | MemberDecl::ClassField {
                class: DeclRef::External(_),
                ..
            }
            | MemberDecl::InterfaceField {
                interface: DeclRef::External(_),
                ..
            }
            | MemberDecl::EnumVariant {
                enum_loc: DeclRef::External(_),
                ..
            } => None,
        };
        let item = Completion {
            label: candidate.name.as_str().to_string(),
            source_range: self.source_range,
            insert: insert_for_name(&candidate.name, candidate.is_method),
            kind: match (candidate.is_method, &candidate.decl) {
                (true, _) => CompletionKind::Method,
                (false, MemberDecl::EnumVariant { .. }) => CompletionKind::EnumVariant,
                (false, _) => CompletionKind::Field,
            },
            detail,
            documentation,
            relevance: CompletionRelevance {
                is_inherent: matches!(candidate.source, MemberSource::Inherent),
                ..CompletionRelevance::default()
            },
        };
        self.push(declared_in, item);
    }

    /// An item or child namespace reached through a package qualifier.
    /// Everything a qualifier reaches is equally "in" it — the reader
    /// narrowed the space by writing the qualifier — so relevance is flat.
    pub(super) fn add_namespace_member(&mut self, member: &NamespaceMember<'_>) {
        let (kind, detail, documentation) = match &member.kind {
            NamespaceMemberKind::Item(def) => {
                let (detail, documentation) = render::definition(self.db, self.file, def);
                (definition_kind(def.kind()), detail, documentation)
            }
            NamespaceMemberKind::Namespace => {
                (CompletionKind::Package, Some("namespace".to_string()), None)
            }
        };
        let item = Completion {
            label: member.name.as_str().to_string(),
            source_range: self.source_range,
            insert: insert_for_name(&member.name, kind == CompletionKind::Function),
            kind,
            detail,
            documentation,
            relevance: CompletionRelevance::default(),
        };
        // A child namespace is a path component, not a declaration; only
        // the items in it can carry the mark.
        let declared_in = match &member.kind {
            NamespaceMemberKind::Item(def) => Some(def.file(self.db)),
            NamespaceMemberKind::Namespace => None,
        };
        self.push(declared_in, item);
    }

    /// A bare name in scope: a local, an own-package item, or a dependency
    /// package's name.
    pub(super) fn add_scope_name(&mut self, entry: &ScopeName<'_>) {
        let (kind, is_local, is_own_package) = match &entry.kind {
            ScopeNameKind::Local { .. } => (CompletionKind::Local, true, false),
            ScopeNameKind::Item(def) => (definition_kind(def.kind()), false, true),
            ScopeNameKind::Package => (CompletionKind::Package, false, false),
        };
        let (detail, documentation) = match &entry.kind {
            ScopeNameKind::Item(def) => render::definition(self.db, self.file, def),
            // A local's type is the inferred one, which hover already
            // renders; the list stays quiet rather than restating a guess.
            ScopeNameKind::Local { .. } | ScopeNameKind::Package => (None, None),
        };
        // A bare name reaches the file's own namespace and nothing else, so
        // an item here is the reader's by construction; a local is theirs by
        // definition, and a package NAME is the only spelling that reaches
        // the package at all.
        let item = Completion {
            label: entry.name.as_str().to_string(),
            source_range: self.source_range,
            insert: insert_for_name(&entry.name, kind == CompletionKind::Function),
            kind,
            detail,
            documentation,
            relevance: CompletionRelevance {
                is_local,
                is_own_package,
                ..CompletionRelevance::default()
            },
        };
        self.push(None, item);
    }

    /// A bare name that resolves as a TYPE: an own-namespace type, a
    /// generic parameter, or a package rooting a qualified type path.
    pub(super) fn add_type_scope_name(&mut self, entry: &TypeScopeName<'_>) {
        let (kind, is_local, is_own_package) = match &entry.kind {
            TypeScopeNameKind::Item(def) => (definition_kind(def.kind()), false, true),
            // A generic parameter is the type-side analogue of a local:
            // the reader (or the item they are inside) just declared it.
            TypeScopeNameKind::GenericParam => (CompletionKind::TypeParam, true, false),
            TypeScopeNameKind::Package => (CompletionKind::Package, false, false),
        };
        let (detail, documentation) = match &entry.kind {
            TypeScopeNameKind::Item(def) => render::definition(self.db, self.file, def),
            TypeScopeNameKind::GenericParam | TypeScopeNameKind::Package => (None, None),
        };
        // Own-namespace types, the reader's own generic parameters, and
        // package names — the same three the expression side offers.
        let item = Completion {
            label: entry.name.as_str().to_string(),
            source_range: self.source_range,
            insert: CompletionInsert::Plain(entry.name.as_str().to_string()),
            kind,
            detail,
            documentation,
            relevance: CompletionRelevance {
                is_local,
                is_own_package,
                ..CompletionRelevance::default()
            },
        };
        self.push(None, item);
    }

    /// A builtin type alias (`int`, `string`, `json`) — the language's own
    /// spelling table, offered wherever a type can be written.
    pub(super) fn add_builtin_type(&mut self, alias: &str) {
        let item = Completion {
            label: alias.to_string(),
            source_range: self.source_range,
            insert: CompletionInsert::Plain(alias.to_string()),
            kind: CompletionKind::BuiltinType,
            detail: None,
            documentation: baml_builtins2::language_topic(alias).map(|topic| topic.summary.clone()),
            relevance: CompletionRelevance::default(),
        };
        self.push(None, item);
    }

    /// A declaration keyword with its skeleton: accepting `class` writes
    /// `class Name { … }` with tab stops, not the bare word. The label and
    /// filter stay the keyword, so typing narrows exactly as before.
    pub(super) fn add_declaration(&mut self, keyword: &str, snippet: &str) {
        let item = Completion {
            label: keyword.to_string(),
            source_range: self.source_range,
            insert: CompletionInsert::Snippet(snippet.to_string()),
            kind: CompletionKind::Keyword,
            detail: None,
            documentation: baml_builtins2::language_topic(keyword)
                .map(|topic| topic.summary.clone()),
            relevance: CompletionRelevance::default(),
        };
        self.push(None, item);
    }

    /// An `@attribute` name the compiler gives meaning to.
    pub(super) fn add_attribute(&mut self, name: &str) {
        let item = Completion {
            label: name.to_string(),
            source_range: self.source_range,
            insert: CompletionInsert::Plain(name.to_string()),
            kind: CompletionKind::Attribute,
            detail: None,
            documentation: baml_builtins2::language_topic(name).map(|topic| topic.summary.clone()),
            relevance: CompletionRelevance::default(),
        };
        self.push(None, item);
    }

    /// A keyword that can open a form the grammar accepts at the position.
    pub(super) fn add_keyword(&mut self, keyword: &str) {
        let item = Completion {
            label: keyword.to_string(),
            source_range: self.source_range,
            insert: CompletionInsert::Plain(keyword.to_string()),
            kind: CompletionKind::Keyword,
            detail: None,
            documentation: baml_builtins2::language_topic(keyword)
                .map(|topic| topic.summary.clone()),
            relevance: CompletionRelevance::default(),
        };
        self.push(None, item);
    }

    /// An argument label in the call it belongs to — the most specific
    /// thing an argument slot can be, which the relevance says.
    pub(super) fn add_argument_label(&mut self, name: &Name, ty: &baml_type::Ty) {
        // A named-only parameter's name IS the calling convention: hiding it
        // would leave the parameter unpassable, so a label is never a
        // package's internal.
        let item = Completion {
            label: name.as_str().to_string(),
            source_range: self.source_range,
            // `name = ` — the value is what comes next, so the caret lands
            // after the equals rather than inside a placeholder.
            insert: CompletionInsert::Plain(format!("{} = ", name.as_str())),
            kind: CompletionKind::Parameter,
            detail: Some(crate::render::display_ty_canonical_for_file(
                self.db, self.file, ty,
            )),
            documentation: None,
            relevance: CompletionRelevance {
                is_parameter: true,
                ..CompletionRelevance::default()
            },
        };
        self.push(None, item);
    }

    /// An unwritten field in an object literal, inserted ready for its
    /// value.
    pub(super) fn add_record_field(
        &mut self,
        class: baml_compiler2_hir::loc::ClassLoc<'_>,
        field: &baml_compiler2_hir::item_data::FieldData,
        ty: Option<&baml_type::Ty>,
    ) {
        let item = Completion {
            label: field.name.as_str().to_string(),
            source_range: self.source_range,
            insert: CompletionInsert::Plain(format!("{}: ", field.name.as_str())),
            kind: CompletionKind::Field,
            detail: ty
                .map(|ty| crate::render::display_ty_canonical_for_file(self.db, self.file, ty)),
            documentation: field.docstring.clone(),
            relevance: CompletionRelevance {
                is_inherent: true,
                ..CompletionRelevance::default()
            },
        };
        self.push(Some(class.file(self.db)), item);
    }

    /// `declared_in` is the source file the offered name is declared in,
    /// or `None` when it is not a source declaration at all — a local, a
    /// generic parameter, a keyword, a builtin spelling, a package name, an
    /// argument label, a mounted package's exported row.
    fn push(&mut self, declared_in: Option<SourceFile>, item: Completion) {
        if self.is_hidden(&item, declared_in) {
            return;
        }
        self.items.push(item);
    }

    /// Whether the stdlib's `_` convention keeps this name out of the list.
    ///
    /// The policy is [`crate::symbols::Internals`], which describe's search
    /// and listings take too; this is the one place completion reads it, so
    /// every provider obeys at once.
    fn is_hidden(&self, item: &Completion, declared_in: Option<SourceFile>) -> bool {
        declared_in.is_some_and(|file| self.internals.hides(self.db, &item.label, file))
    }
}

/// A callable inserts its call with the tab stop between the parentheses,
/// so the next keystroke is the first argument; everything else inserts its
/// name.
fn insert_for_name(name: &Name, callable: bool) -> CompletionInsert {
    if callable {
        CompletionInsert::Snippet(format!("{}($0)", name.as_str()))
    } else {
        CompletionInsert::Plain(name.as_str().to_string())
    }
}

/// The ONE mapping from an item's definition kind to its completion kind
/// (the providers previously kept two, which had already drifted on `Let`).
fn definition_kind(kind: DefinitionKind) -> CompletionKind {
    match kind {
        DefinitionKind::Function => CompletionKind::Function,
        DefinitionKind::Class => CompletionKind::Class,
        DefinitionKind::Enum => CompletionKind::Enum,
        DefinitionKind::Interface => CompletionKind::Interface,
        DefinitionKind::TypeAlias => CompletionKind::TypeAlias,
        DefinitionKind::Client => CompletionKind::Client,
        DefinitionKind::Let => CompletionKind::Local,
        DefinitionKind::Method => CompletionKind::Method,
        DefinitionKind::Field => CompletionKind::Field,
        DefinitionKind::Variant => CompletionKind::EnumVariant,
        DefinitionKind::Parameter => CompletionKind::Parameter,
        DefinitionKind::Binding => CompletionKind::Local,
        DefinitionKind::AssociatedType => CompletionKind::Other,
    }
}
