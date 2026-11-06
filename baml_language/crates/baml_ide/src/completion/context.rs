//! Classifying the cursor: what kind of position is this, and what does the
//! qualifier before it mean?
//!
//! The classification runs on a speculative parse — the file's text with a
//! marker identifier spliced in at the cursor — for the reason rust-analyzer
//! does the same: at a completion position the real text is usually not
//! valid, and a tree parsed from it labels the position by how the parser
//! recovered rather than by what the user is writing. `function f() -> ⎸ {`
//! parses with an EMPTY type node the cursor cannot be found in; with a
//! marker spliced in it is an ordinary type reference.
//!
//! Only offsets STRICTLY BEFORE the cursor are shared between the two texts.
//! The classification hands the RESOLUTION step such offsets, and the
//! resolution reads the real file's recorded facts — which is what makes it
//! safe to classify on one tree and resolve on the other.
//!
//! Resolution happens HERE, not in a provider: deciding what the `.` before
//! the cursor reads from is analysis (rust-analyzer's
//! `Qualified::With { resolution }`), and a provider that had to try the
//! readings itself would be re-deriving the position it was handed.

use baml_base::{AttributePosition, SourceFile};
use baml_compiler2_hir::{
    body::BodyOwnerId,
    contributions::Definition,
    resolve::{NamespaceMember, namespace_members_at},
};
use baml_syntax::{
    index::TreeIndex,
    nodes::{
        Arg, ArgList, ArrayType, Attribute, BacktickString, Block, BlockAttr, ClassDef, EnumDef,
        EnumVariant, Field, FileTree, FunctionType, ImplementsBlock, ImplementsFor, InterfaceDef,
        LiteralType, Name, ObjectLiteral, OptionalType, ParenType, PathType, QualifiedPath,
        SourceFile as SourceFileNode, TemplateElif, TemplateForOpen, TemplateIfOpen,
        TemplateInterp, UnionType,
    },
    token::{TokenAtOffset, TokenIdx, TokenKind, TokenSet as _, sets::StringToken},
    tree::{AnyNodeId, Bogus},
};
use text_size::{TextRange, TextSize};

use crate::{resolve, syntax};

/// The identifier spliced in at the cursor. A plausible identifier (so the
/// parser takes the branch the user is heading for) that no real source would
/// contain (so a classification can find it unambiguously).
const MARKER: &str = "bamlCompletionMarker";

/// Where the cursor is, in the terms providers need.
pub(crate) struct CompletionContext<'db> {
    /// The range an item replaces: the identifier fragment already typed,
    /// empty when the cursor sits right after a `.`.
    pub(crate) source_range: TextRange,
    pub(crate) analysis: CompletionAnalysis<'db>,
}

/// The kind of position, with everything about it already resolved.
pub(crate) enum CompletionAnalysis<'db> {
    /// A name is being written: `<here>`, or `<qualifier>.<here>`.
    Path {
        kind: PathKind,
        /// The resolved dot before the cursor, when there is one. A `None`
        /// qualifier is a bare name.
        qualifier: Option<DotTarget<'db>>,
    },
    /// An argument slot: `f(<here>)`, `f(1, <here>)`. Carries the callee's
    /// resolved position; a slot whose call the checker recorded nothing
    /// for classifies as a bare expression instead.
    CallArgument { call: resolve::CallPosition },
    /// A field slot in an object literal: `Foo { <here> }`, resolved to the
    /// class inference recorded for the literal.
    RecordField {
        literal: resolve::ObjectLiteralPosition<'db>,
    },
    /// An item is being declared: `⎸` at the top of a file or inside a
    /// class/interface/implements body — the declaration keywords the
    /// grammar accepts there.
    Item { container: ItemContainer },
    /// An `@attribute` name at a declaration position.
    Attribute { position: AttributePosition },
    /// A position no provider knows yet. Additive by construction: an
    /// unclassified position offers nothing rather than guessing.
    Unsupported,
}

/// Which body an item declaration is being written in, which decides the
/// keyword vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ItemContainer {
    TopLevel,
    Class,
    Interface,
    Impl,
}

/// What a written name at the position IS. The kind selects which view of a
/// [`DotTarget`] a provider offers — the same qualifier means different
/// members in expression and type position — so C3's type positions are a
/// new variant here, not a second qualifier story.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathKind {
    /// An expression: locals, items, packages, and the keywords that can
    /// start one.
    Expr,
    /// A type is being written: an annotation, a parameter/return type, a
    /// type argument, a bound, a `throws` clause — or a match/catch arm,
    /// because a pattern IS a type (`TYPE_PATTERN` wraps a `TYPE_EXPR`;
    /// BEP-015's membership model makes that literal).
    Type,
    // Item positions arrive with C4.
}

/// What the `.` before the cursor reads a member OF, resolved once.
///
/// BAML's `.` is overloaded three ways where Rust spells two tokens (`.`
/// and `::`), so the one thing every dotted completion needs decided is
/// which reading the prefix takes. The readings are tried in resolution's
/// own order: a typed VALUE claims first, then a TYPE declaration, then a
/// NAMESPACE — and a prefix none of them claim offers nothing.
pub(crate) enum DotTarget<'db> {
    /// A value receiver: `p.` — its inferred type, in the body owner's
    /// param env (a `T extends Compare` receiver has members only because
    /// the owner declared that bound).
    Value {
        owner: BodyOwnerId<'db>,
        receiver: baml_type::Ty,
    },
    /// A type qualifier: `Point.`, `int.`, `baml.iter.Range.` — the
    /// declaration it names.
    Type(Definition<'db>),
    /// A namespace qualifier: `baml.`, `baml.http.`, `root.ns.`.
    ///
    /// Carries the members rather than the written chain because resolving
    /// a namespace and enumerating it are one query — `namespace_members_at`
    /// answering `None` is precisely what distinguishes a qualifier from a
    /// value — so the target holds what resolution already had in hand.
    Namespace(Vec<NamespaceMember<'db>>),
}

impl<'db> CompletionContext<'db> {
    pub(crate) fn new(
        db: &'db dyn baml_compiler2_hir::Db,
        file: SourceFile,
        offset: TextSize,
    ) -> Option<Self> {
        let text = file.text(db);
        if usize::from(offset) > text.len() {
            return None;
        }
        let spec = speculative_parse(text, offset)?;
        let tokens = spec.tree.tokens();
        let token = match tokens.token_at_offset(spec.marker.start() + TextSize::from(1)) {
            TokenAtOffset::Single(token) => token,
            TokenAtOffset::Between { right, .. } => right,
            TokenAtOffset::None => return None,
        };

        // The fragment already typed is the marker token minus the marker
        // itself — real coordinates, because it ends at the cursor.
        let source_range = if tokens.kind(token) == TokenKind::Word {
            TextRange::new(tokens.range(token).start().min(offset), offset)
        } else {
            TextRange::empty(offset)
        };

        // Classify on the speculative tree, then resolve what the
        // classification located against the real file's recorded facts.
        let position = classify(&spec, token);
        // An accepted attribute replaces the WHOLE name: `@stream.⎸` is one
        // dotted attribute name, and the fragment-only range would splice
        // `stream.stream.done`.
        let source_range = match &position {
            Position::Attribute { name_start, .. } => {
                TextRange::new((*name_start).min(offset), offset)
            }
            _ => source_range,
        };
        let analysis = match position {
            Position::Dotted { dot, kind } => match dot_target(db, file, dot) {
                Some(qualifier) => CompletionAnalysis::Path {
                    kind,
                    qualifier: Some(qualifier),
                },
                None => CompletionAnalysis::Unsupported,
            },
            Position::Type => CompletionAnalysis::Path {
                kind: PathKind::Type,
                qualifier: None,
            },
            // A slot whose call inference recorded nothing for is still an
            // expression position; only the labels are gone.
            Position::ArgumentSlot { open_paren } => match resolve::call_at(db, file, open_paren) {
                Some(call) => CompletionAnalysis::CallArgument { call },
                None => CompletionAnalysis::Path {
                    kind: PathKind::Expr,
                    qualifier: None,
                },
            },
            Position::FieldSlot { literal } => {
                match resolve::object_literal_at(db, file, literal) {
                    Some(literal) => CompletionAnalysis::RecordField { literal },
                    None => CompletionAnalysis::Unsupported,
                }
            }
            Position::Expression => CompletionAnalysis::Path {
                kind: PathKind::Expr,
                qualifier: None,
            },
            Position::Item { container } => CompletionAnalysis::Item { container },
            Position::Attribute { position, .. } => CompletionAnalysis::Attribute { position },
            Position::Unsupported => CompletionAnalysis::Unsupported,
        };

        Some(Self {
            source_range,
            analysis,
        })
    }
}

/// The speculative parse: the file's text with [`MARKER`] spliced in at the
/// cursor, its tree, the index over it, and the marker's range within it.
struct Speculative {
    tree: FileTree,
    index: TreeIndex,
    marker: TextRange,
}

impl Speculative {
    fn node_range(&self, id: AnyNodeId) -> TextRange {
        self.tree.tokens().text_range(self.tree.arenas().span(id))
    }

    /// The nodes from the token's owner up to the root; none for trivia.
    fn ancestors_of(&self, token: TokenIdx) -> impl Iterator<Item = AnyNodeId> + '_ {
        self.index
            .token_owner(token)
            .into_iter()
            .flat_map(|owner| self.index.ancestors(owner))
    }
}

/// Parse the file's text with [`MARKER`] spliced in at `offset`.
fn speculative_parse(text: &str, offset: TextSize) -> Option<Speculative> {
    let at = usize::from(offset);
    if !text.is_char_boundary(at) {
        return None;
    }
    let mut faked = String::with_capacity(text.len() + MARKER.len());
    faked.push_str(&text[..at]);
    faked.push_str(MARKER);
    faked.push_str(&text[at..]);
    let tree = baml_parser::parse(&faked).tree;
    let index = TreeIndex::build(&tree);
    Some(Speculative {
        tree,
        index,
        marker: TextRange::at(offset, TextSize::of(MARKER)),
    })
}

/// The purely syntactic classification, before any resolution. Every offset
/// it carries lies strictly before the cursor, so it is valid in the real
/// file too.
enum Position {
    /// `<something>.<here>` — `dot` is where the `.` begins, which is also
    /// where the prefix's recorded span ends.
    Dotted {
        dot: TextSize,
        kind: PathKind,
    },
    /// A bare name in type position.
    Type,
    /// `f(<here>)` — `open_paren` is where the argument list begins, which
    /// is where the callee's span ends.
    ArgumentSlot {
        open_paren: TextSize,
    },
    /// `Foo { <here> }` — `literal` is an offset inside the literal's
    /// recorded span.
    FieldSlot {
        literal: TextSize,
    },
    /// An item declaration slot.
    Item {
        container: ItemContainer,
    },
    /// An `@attribute` name; `name_start` is where the name begins (after
    /// the `@`/`@@`), so accepting a dotted name replaces the whole fragment.
    Attribute {
        name_start: TextSize,
        position: AttributePosition,
    },
    Expression,
    Unsupported,
}

/// The declaration an attribute annotates, or `None` where the grammar
/// accepts an attribute the schema table has no position for.
fn attribute_position(
    spec: &Speculative,
    attribute: AnyNodeId,
    block: bool,
) -> Option<AttributePosition> {
    let mut owners = spec.index.ancestors(attribute).skip(1);
    if block {
        // A `@@` is a member of the declaration it describes.
        return owners.find_map(|owner| {
            if owner.cast::<ClassDef>().is_some() {
                Some(AttributePosition::Class)
            } else if owner.cast::<EnumDef>().is_some() {
                Some(AttributePosition::Enum)
            } else if owner.cast::<InterfaceDef>().is_some() {
                Some(AttributePosition::Interface)
            } else {
                None
            }
        });
    }
    owners.find_map(|owner| {
        if owner.cast::<Field>().is_some() {
            // A field is a class's or an interface's; the enclosing
            // declaration says which.
            spec.index.ancestors(owner).skip(1).find_map(|declaration| {
                if declaration.cast::<ClassDef>().is_some() {
                    Some(AttributePosition::ClassField)
                } else if declaration.cast::<InterfaceDef>().is_some() {
                    Some(AttributePosition::InterfaceField)
                } else {
                    None
                }
            })
        } else if owner.cast::<EnumVariant>().is_some() {
            Some(AttributePosition::EnumVariant)
        } else {
            None
        }
    })
}

/// Whether `id` is a type expression node.
fn is_type_node(id: AnyNodeId) -> bool {
    id.cast::<PathType>().is_some()
        || id.cast::<LiteralType>().is_some()
        || id.cast::<ParenType>().is_some()
        || id.cast::<FunctionType>().is_some()
        || id.cast::<OptionalType>().is_some()
        || id.cast::<ArrayType>().is_some()
        || id.cast::<UnionType>().is_some()
        || id.cast::<QualifiedPath>().is_some()
}

/// What the marker token's surroundings say the position is.
///
/// Innermost-first: an object literal inside an argument list is a field
/// slot, not an argument slot, because that is the node the cursor is in.
fn classify(spec: &Speculative, token: TokenIdx) -> Position {
    let tokens = spec.tree.tokens();
    if in_prose(spec, token) {
        return Position::Unsupported;
    }
    // A marker inside a type node is a type being written, wherever the
    // expression sits — annotation, signature, bound, `throws`, or a
    // match/catch arm (a type pattern holds a type). The kind rides along
    // on the dotted form: `baml.⎸` in a type slot resolves through the same
    // DotTarget and filters to types. BEFORE the type check: an `@name`
    // sits on the annotated member, and before the dot check: `@stream.⎸`
    // is one attribute name, not a qualified path. An attribute at a
    // position the schema table does not know offers nothing.
    let attribute = spec.ancestors_of(token).find_map(|id| {
        if id.cast::<Attribute>().is_some() {
            Some((id, false))
        } else if id.cast::<BlockAttr>().is_some() {
            Some((id, true))
        } else {
            None
        }
    });
    if let Some((attribute, block)) = attribute {
        let Some(position) = attribute_position(spec, attribute, block) else {
            return Position::Unsupported;
        };
        let range = spec.node_range(attribute);
        let ats = spec.tree.text()[range]
            .bytes()
            .take_while(|b| *b == b'@')
            .count();
        // At most two `@`s can lead an attribute; the cast cannot truncate.
        return Position::Attribute {
            name_start: range.start() + TextSize::from(u32::try_from(ats).unwrap_or(u32::MAX)),
            position,
        };
    }
    // An `@`/`@@` the grammar gave no attribute node — a `@@` before a
    // declaration, which takes none — names nothing to offer.
    if tokens
        .prev_significant(token)
        .is_some_and(|previous| matches!(tokens.kind(previous), TokenKind::At | TokenKind::AtAt))
    {
        return Position::Unsupported;
    }
    let in_type = spec.ancestors_of(token).any(is_type_node);
    if let Some(dot) = preceding_dot(spec, token) {
        return Position::Dotted {
            dot,
            kind: if in_type {
                PathKind::Type
            } else {
                PathKind::Expr
            },
        };
    }
    if in_type {
        return Position::Type;
    }
    for node in spec.ancestors_of(token) {
        if node.cast::<ObjectLiteral>().is_some() {
            return Position::FieldSlot {
                literal: spec.node_range(node).start(),
            };
        }
        // The VALUE of an already-labelled argument (`tag = <here>`) is an
        // expression; the label is spoken for.
        if let Some(arg) = node.cast::<Arg>()
            && labelled_before(spec, arg, token)
        {
            return Position::Expression;
        }
        if node.cast::<ArgList>().is_some() {
            return Position::ArgumentSlot {
                open_paren: spec.node_range(node).start(),
            };
        }
        // A block, or a template's `${…}` hole and block headers: code.
        if node.cast::<Block>().is_some()
            || node.cast::<TemplateInterp>().is_some()
            || node.cast::<TemplateForOpen>().is_some()
            || node.cast::<TemplateIfOpen>().is_some()
            || node.cast::<TemplateElif>().is_some()
        {
            return Position::Expression;
        }
    }
    // Item slots: a bare word the parser could only read as the start of a
    // declaration (top level: a `Bogus` item) or as a field name (class-like
    // bodies — where a declaration keyword is equally writable).
    if tokens.kind(token) == TokenKind::Word
        && let Some(owner) = spec.index.token_owner(token)
    {
        let parent = spec.index.parent(owner);
        let container = if owner.cast::<Bogus>().is_some() {
            parent.and_then(|parent| {
                if parent.cast::<SourceFileNode>().is_some() {
                    Some(ItemContainer::TopLevel)
                } else if parent.cast::<ImplementsFor>().is_some()
                    || parent.cast::<ImplementsBlock>().is_some()
                {
                    Some(ItemContainer::Impl)
                } else {
                    None
                }
            })
        } else if owner.cast::<Name>().is_some()
            && parent.is_some_and(|parent| parent.cast::<Field>().is_some())
        {
            spec.index.ancestors(owner).find_map(|node| {
                if node.cast::<ClassDef>().is_some() {
                    Some(ItemContainer::Class)
                } else if node.cast::<InterfaceDef>().is_some() {
                    Some(ItemContainer::Interface)
                } else {
                    None
                }
            })
        } else {
            None
        };
        if let Some(container) = container {
            return Position::Item { container };
        }
    }
    Position::Unsupported
}

/// The three readings of the `.` at `dot`, tried in resolution's own order.
fn dot_target(
    db: &dyn baml_compiler2_hir::Db,
    file: SourceFile,
    dot: TextSize,
) -> Option<DotTarget<'_>> {
    if let Some((owner, receiver)) = resolve::receiver_at_dot(db, file, dot) {
        return Some(DotTarget::Value { owner, receiver });
    }
    let chain = qualifier_chain(db, file, dot)?;
    if let Some(definition) = resolve::type_qualifier_at(db, file, &chain) {
        return Some(DotTarget::Type(definition));
    }
    namespace_members_at(db, file, &chain).map(DotTarget::Namespace)
}

/// The dotted prefix the reader has already written, left of `dot` — read
/// from the REAL file (the chain ends before the cursor), never from the
/// fragment being typed.
fn qualifier_chain(
    db: &dyn baml_compiler2_hir::Db,
    file: SourceFile,
    dot: TextSize,
) -> Option<Vec<baml_base::Name>> {
    let token = syntax::find_token_at_offset(db, file, dot)?;
    let last = token.prev().filter(syntax::TokenRef::is_ident)?;
    Some(syntax::dotted_chain_to(&last))
}

/// Whether this argument already carries its label, i.e. there is an `=`
/// between the argument's start and the cursor.
fn labelled_before(spec: &Speculative, arg: baml_syntax::tree::Id<Arg>, token: TokenIdx) -> bool {
    let tokens = spec.tree.tokens();
    spec.tree
        .get(arg)
        .label
        .is_some_and(|label| tokens.range(label.eq.idx).end() <= tokens.range(token).start())
}

/// Whether the cursor sits in text that is not code: a comment, or the
/// literal part of a string.
///
/// A backtick string is both — its `${…}` interpolations ARE code — so the
/// walk stops at whichever comes first: an interpolation (code, keep going)
/// or the string itself (prose, stop).
fn in_prose(spec: &Speculative, token: TokenIdx) -> bool {
    let kind = spec.tree.tokens().kind(token);
    if matches!(
        kind,
        TokenKind::LineComment
            | TokenKind::BlockComment
            | TokenKind::UnclosedBlockComment
            | TokenKind::DocComment
            | TokenKind::DirectiveComment
            | TokenKind::HeaderComment
            | TokenKind::TemplateChunk
    ) || StringToken::KINDS.contains(kind)
    {
        return true;
    }
    for node in spec.ancestors_of(token) {
        if node.cast::<TemplateInterp>().is_some()
            || node.cast::<TemplateForOpen>().is_some()
            || node.cast::<TemplateIfOpen>().is_some()
            || node.cast::<TemplateElif>().is_some()
        {
            return false;
        }
        if node.cast::<BacktickString>().is_some() {
            return true;
        }
    }
    false
}

/// The `.` immediately before `token` — plain or optional-chaining, since
/// `a?.b` reads a member of `a` exactly as `a.b` does. Trivia between the
/// two is skipped: a dot on its own line is a member access being written
/// across lines, and the receiver is still the value before it.
fn preceding_dot(spec: &Speculative, token: TokenIdx) -> Option<TextSize> {
    let tokens = spec.tree.tokens();
    let previous = tokens.prev_significant(token)?;
    matches!(
        tokens.kind(previous),
        TokenKind::Dot | TokenKind::QuestionDot
    )
    .then(|| tokens.range(previous).start())
}
