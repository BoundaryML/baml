//! The accumulator (rust-analyzer's `Completions`): providers say WHAT is
//! offered, and this module is the one place that decides HOW an offer is
//! presented — its insert text, its kind, and the relevance facts it ranks
//! by. A presentation rule stated here holds for every provider at once,
//! which is exactly what the hand-built item literals this replaces could
//! not promise.

use baml_base::{Name, SourceFile};
use baml_compiler2_hir::contributions::DefinitionKind;
use baml_compiler2_hir_ty::method_resolution::{MemberCandidate, MemberDecl, MemberSource};
use baml_compiler2_ppir::resolve::{
    NamespaceMember, NamespaceMemberKind, ScopeName, ScopeNameKind, TypeScopeName,
    TypeScopeNameKind,
};
use text_size::TextRange;

use super::{
    item::{Completion, CompletionInsert, CompletionKind, CompletionRelevance},
    render,
    render::MemberForm,
};

pub(super) struct Completions {
    /// The range every accepted item replaces: the fragment already typed.
    source_range: TextRange,
    items: Vec<Completion>,
}

impl Completions {
    pub(super) fn new(source_range: TextRange) -> Self {
        Self {
            source_range,
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
    pub(super) fn add_member(
        &mut self,
        db: &dyn baml_compiler2_ppir::Db,
        file: SourceFile,
        candidate: &MemberCandidate<'_>,
        form: MemberForm,
    ) {
        let (detail, documentation) = render::member(db, file, &candidate.decl, form);
        self.push(Completion {
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
        });
    }

    /// An item or child namespace reached through a package qualifier.
    /// Everything a qualifier reaches is equally "in" it — the reader
    /// narrowed the space by writing the qualifier — so relevance is flat.
    pub(super) fn add_namespace_member(
        &mut self,
        db: &dyn baml_compiler2_ppir::Db,
        file: SourceFile,
        member: &NamespaceMember<'_>,
    ) {
        let (kind, detail, documentation) = match &member.kind {
            NamespaceMemberKind::Item(def) => {
                let (detail, documentation) = render::definition(db, file, def);
                (definition_kind(def.kind()), detail, documentation)
            }
            NamespaceMemberKind::Namespace => {
                (CompletionKind::Package, Some("namespace".to_string()), None)
            }
        };
        self.push(Completion {
            label: member.name.as_str().to_string(),
            source_range: self.source_range,
            insert: insert_for_name(&member.name, kind == CompletionKind::Function),
            kind,
            detail,
            documentation,
            relevance: CompletionRelevance::default(),
        });
    }

    /// A bare name in scope: a local, an own-package item, or a dependency
    /// package's name.
    pub(super) fn add_scope_name(
        &mut self,
        db: &dyn baml_compiler2_ppir::Db,
        file: SourceFile,
        entry: &ScopeName<'_>,
    ) {
        let (kind, is_local, is_own_package) = match &entry.kind {
            ScopeNameKind::Local { .. } => (CompletionKind::Local, true, false),
            ScopeNameKind::Item(def) => (definition_kind(def.kind()), false, true),
            ScopeNameKind::Package => (CompletionKind::Package, false, false),
        };
        let (detail, documentation) = match &entry.kind {
            ScopeNameKind::Item(def) => render::definition(db, file, def),
            // A local's type is the inferred one, which hover already
            // renders; the list stays quiet rather than restating a guess.
            ScopeNameKind::Local { .. } | ScopeNameKind::Package => (None, None),
        };
        self.push(Completion {
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
        });
    }

    /// A bare name that resolves as a TYPE: an own-namespace type, a
    /// generic parameter, or a package rooting a qualified type path.
    pub(super) fn add_type_scope_name(
        &mut self,
        db: &dyn baml_compiler2_ppir::Db,
        file: SourceFile,
        entry: &TypeScopeName<'_>,
    ) {
        let (kind, is_local, is_own_package) = match &entry.kind {
            TypeScopeNameKind::Item(def) => (definition_kind(def.kind()), false, true),
            // A generic parameter is the type-side analogue of a local:
            // the reader (or the item they are inside) just declared it.
            TypeScopeNameKind::GenericParam => (CompletionKind::TypeParam, true, false),
            TypeScopeNameKind::Package => (CompletionKind::Package, false, false),
        };
        let (detail, documentation) = match &entry.kind {
            TypeScopeNameKind::Item(def) => render::definition(db, file, def),
            TypeScopeNameKind::GenericParam | TypeScopeNameKind::Package => (None, None),
        };
        self.push(Completion {
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
        });
    }

    /// A builtin type alias (`int`, `string`, `json`) — the language's own
    /// spelling table, offered wherever a type can be written.
    pub(super) fn add_builtin_type(&mut self, alias: &str) {
        self.push(Completion {
            label: alias.to_string(),
            source_range: self.source_range,
            insert: CompletionInsert::Plain(alias.to_string()),
            kind: CompletionKind::BuiltinType,
            detail: None,
            documentation: baml_builtins2::language_topic(alias).map(|topic| topic.summary.clone()),
            relevance: CompletionRelevance::default(),
        });
    }

    /// A declaration keyword with its skeleton: accepting `class` writes
    /// `class Name { … }` with tab stops, not the bare word. The label and
    /// filter stay the keyword, so typing narrows exactly as before.
    pub(super) fn add_declaration(&mut self, keyword: &str, snippet: &str) {
        self.push(Completion {
            label: keyword.to_string(),
            source_range: self.source_range,
            insert: CompletionInsert::Snippet(snippet.to_string()),
            kind: CompletionKind::Keyword,
            detail: None,
            documentation: baml_builtins2::language_topic(keyword)
                .map(|topic| topic.summary.clone()),
            relevance: CompletionRelevance::default(),
        });
    }

    /// An `@attribute` name the compiler gives meaning to.
    pub(super) fn add_attribute(&mut self, name: &str) {
        self.push(Completion {
            label: name.to_string(),
            source_range: self.source_range,
            insert: CompletionInsert::Plain(name.to_string()),
            kind: CompletionKind::Attribute,
            detail: None,
            documentation: baml_builtins2::language_topic(name).map(|topic| topic.summary.clone()),
            relevance: CompletionRelevance::default(),
        });
    }

    /// A keyword that can open a form the grammar accepts at the position.
    pub(super) fn add_keyword(&mut self, keyword: &str) {
        self.push(Completion {
            label: keyword.to_string(),
            source_range: self.source_range,
            insert: CompletionInsert::Plain(keyword.to_string()),
            kind: CompletionKind::Keyword,
            detail: None,
            documentation: baml_builtins2::language_topic(keyword)
                .map(|topic| topic.summary.clone()),
            relevance: CompletionRelevance::default(),
        });
    }

    /// An argument label in the call it belongs to — the most specific
    /// thing an argument slot can be, which the relevance says.
    pub(super) fn add_argument_label(
        &mut self,
        db: &dyn baml_compiler2_ppir::Db,
        file: SourceFile,
        name: &Name,
        ty: &baml_type::interned::Ty,
    ) {
        self.push(Completion {
            label: name.as_str().to_string(),
            source_range: self.source_range,
            // `name = ` — the value is what comes next, so the caret lands
            // after the equals rather than inside a placeholder.
            insert: CompletionInsert::Plain(format!("{} = ", name.as_str())),
            kind: CompletionKind::Parameter,
            detail: Some(crate::render::display_ty_canonical_for_file(
                db,
                file,
                &ty.to_plain(),
            )),
            documentation: None,
            relevance: CompletionRelevance {
                is_parameter: true,
                ..CompletionRelevance::default()
            },
        });
    }

    /// An unwritten field in an object literal, inserted ready for its
    /// value.
    pub(super) fn add_record_field(
        &mut self,
        db: &dyn baml_compiler2_ppir::Db,
        file: SourceFile,
        field: &baml_compiler2_ppir::item_data::FieldData,
        ty: Option<&baml_type::Ty>,
    ) {
        self.push(Completion {
            label: field.name.as_str().to_string(),
            source_range: self.source_range,
            insert: CompletionInsert::Plain(format!("{}: ", field.name.as_str())),
            kind: CompletionKind::Field,
            detail: ty.map(|ty| crate::render::display_ty_canonical_for_file(db, file, ty)),
            documentation: field.docstring.clone(),
            relevance: CompletionRelevance {
                is_inherent: true,
                ..CompletionRelevance::default()
            },
        });
    }

    fn push(&mut self, item: Completion) {
        self.items.push(item);
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
        DefinitionKind::RetryPolicy => CompletionKind::RetryPolicy,
        DefinitionKind::Let => CompletionKind::Local,
        DefinitionKind::Method => CompletionKind::Method,
        DefinitionKind::Field => CompletionKind::Field,
        DefinitionKind::Variant => CompletionKind::EnumVariant,
        DefinitionKind::Parameter => CompletionKind::Parameter,
        DefinitionKind::Binding => CompletionKind::Local,
        DefinitionKind::TemplateString | DefinitionKind::AssociatedType => CompletionKind::Other,
    }
}
