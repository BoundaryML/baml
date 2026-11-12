//! Declarations and their parts: functions and method signatures, classes,
//! enums, interfaces, implementations, aliases, tests, globals, and the
//! parameter lists, docs and LLM bodies they own. One member per line in a
//! body, `name: T,` for a field, `;` after a signature or associated type,
//! parameters one per line with a trailing comma when the signature breaks.

use baml_syntax::{
    nodes::{
        AnyDoc, AnyFunctionBody, AnyLlmField, AssocTypeDecl, AssocTypeDef, Attribute, ClassDef,
        ClientField, ClientValueDef, DefaultValue, Directive, DocLine, EnumDef, EnumVariant, Field,
        FunctionDef, GenericParamList, HeaderLine, ImplementsBlock, ImplementsFor, InterfaceDef,
        InterfaceFieldLink, LetItem, LlmBody, MethodSig, Name, Param, ParamList, PromptField,
        RequiresClause, ReturnType, TestDef, ThrowsClause, ToolsField, TypeAliasDef,
        TypeAscription, WithClause,
    },
    token::{Token, kw},
    tree::{AnyNodeId, Id, List, NodeUnion as _},
};

use crate::{
    format::{FormatNode, Formatter},
    ir::{Doc, GroupId},
};

// ---------------------------------------------------------------------------
// Leaves
// ---------------------------------------------------------------------------

impl FormatNode for Name {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.token(f.tree.get(id).token)
    }
}

impl FormatNode for DocLine {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.token(f.tree.get(id).token)
    }
}

impl FormatNode for Directive {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.token(f.tree.get(id).token)
    }
}

impl FormatNode for HeaderLine {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.token(f.tree.get(id).token)
    }
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// The signature a function definition and a method signature share.
struct Signature {
    docs: List<AnyDoc>,
    kw: Token<kw::Function>,
    name: Id<Name>,
    generics: Option<Id<GenericParamList>>,
    params: Id<ParamList>,
    ret: Id<ReturnType>,
    throws: Option<Id<ThrowsClause>>,
}

impl Signature {
    /// `function name<G>(params) -> R throws E`, the docs before it, laid
    /// out as rustfmt lays a signature out: all on one line; else the
    /// parameters one per line and `) -> R throws E` together on the closing
    /// line; else `throws E` on a line of its own under it, as a `where`
    /// clause goes. The tail is the named group, so a body can put its `{`
    /// on a line of its own when the tail broke.
    fn doc<'a>(&self, f: &Formatter<'a>) -> (Doc<'a>, Option<GroupId<'a>>) {
        let d = f.docs;
        let (tail, tail_group) = d.group_with_id(d.concat([
            Doc::SPACE,
            f.node(self.ret),
            self.throws.map_or(Doc::NIL, |throws| {
                d.indent(d.concat([Doc::SOFT_LINE_OR_SPACE, f.node(throws)]))
            }),
        ]));
        let signature = d.concat([
            f.docs(self.docs, self.kw.idx),
            f.token(self.kw),
            Doc::SPACE,
            f.node(self.name),
            self.generics.map_or(Doc::NIL, |generics| f.node(generics)),
            signature(f, f.node(self.params), tail),
        ]);
        (signature, self.throws.map(|_| tail_group))
    }
}

/// A parameter list and the tail after it, grouped so that the parameters
/// break before the tail does — unless the tail is sure to break (a line
/// comment ends it), when one group would break the parameters for nothing:
/// the parameters then decide for themselves.
pub(super) fn signature<'a>(f: &Formatter<'a>, params: Doc<'a>, tail: Doc<'a>) -> Doc<'a> {
    let d = f.docs;
    if tail.forced() {
        d.concat([d.group(params), tail])
    } else {
        d.group(d.concat([params, tail]))
    }
}

impl FormatNode for FunctionDef {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let signature = Signature {
            docs: node.docs,
            kw: node.kw,
            name: f.tree.present(node.name),
            generics: node.generics,
            params: f.tree.present(node.params),
            ret: f.tree.present(node.ret),
            throws: node.throws,
        };
        let body = match f.tree.present(node.body) {
            AnyFunctionBody::Expr(block) => f.node(block),
            AnyFunctionBody::Llm(llm) => f.node(llm),
            AnyFunctionBody::Bogus(bogus) => f.tree.absent(bogus),
        };
        // `{` stays on the signature's last line unless a `throws` clause
        // moved under the return type, when it takes a line of its own, as
        // rustfmt puts it after a broken `where` clause. A return type that
        // broke inside its own brackets keeps `> {`.
        let (signature, throws) = signature.doc(f);
        f.docs.concat([signature, f.brace_after(throws), body])
    }
}

impl FormatNode for MethodSig {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let signature = Signature {
            docs: node.docs,
            kw: node.kw,
            name: f.tree.present(node.name),
            generics: node.generics,
            params: f.tree.present(node.params),
            ret: f.tree.present(node.ret),
            throws: node.throws,
        };
        let (signature, _throws) = signature.doc(f);
        f.docs.concat([signature, f.separator(node.semi, ";")])
    }
}

/// `-> T`; a `=>` written in its place is printed as `->`.
impl FormatNode for ReturnType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        d.concat([
            d.text("->"),
            f.comments_after(node.arrow.idx),
            Doc::SPACE,
            f.alt(f.tree.present(node.ty)),
        ])
    }
}

impl FormatNode for ThrowsClause {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs
            .concat([f.token(node.kw), Doc::SPACE, f.alt(f.tree.present(node.ty))])
    }
}

/// `(a: A, b: B)` without a group of its own: the signature or lambda
/// around it groups it with what follows, so that the parameters break
/// before the return type.
impl FormatNode for ParamList {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.delimited_parts(
            node.l,
            f.tree.list(node.params),
            f.tree.present(node.r),
            |param| f.node(param),
        )
    }
}

/// `name: T = default`; `self` has neither.
impl FormatNode for Param {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.node(f.tree.present(node.name)),
            node.ascription
                .map_or(Doc::NIL, |ascription| f.node(ascription)),
            node.default.map_or(Doc::NIL, |default| f.node(default)),
        ])
    }
}

/// `: T`, the colon added where it was left out.
impl FormatNode for TypeAscription {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.separator(node.colon, ":"),
            Doc::SPACE,
            f.alt(f.tree.present(node.ty)),
        ])
    }
}

impl FormatNode for DefaultValue {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            Doc::SPACE,
            f.token(node.eq),
            Doc::SPACE,
            f.alt(f.tree.present(node.value)),
        ])
    }
}

// ---------------------------------------------------------------------------
// LLM bodies
// ---------------------------------------------------------------------------

/// `{ client: … tools: … prompt: … }`, the fields in that order however
/// they were written.
impl FormatNode for LlmBody {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let mut fields = f.tree.list(node.fields).to_vec();
        fields.sort_by_key(|field| match field {
            AnyLlmField::Client(_) => 0,
            AnyLlmField::Tools(_) => 1,
            AnyLlmField::Prompt(_) => 2,
            AnyLlmField::Bogus(bogus) => f.tree.absent(*bogus),
        });
        f.body(AnyNodeId::of(id), node.l, &fields, f.tree.present(node.r))
    }
}

/// `key: value` of an LLM body.
fn llm_field<'a, K: baml_syntax::token::TokenSet>(
    f: &Formatter<'a>,
    kw: Token<K>,
    colon: Result<Token<baml_syntax::token::punct::Colon>, baml_syntax::tree::Missing>,
    value: Doc<'a>,
) -> Doc<'a> {
    f.docs.concat([
        f.token(kw),
        f.token(f.tree.present(colon)),
        Doc::SPACE,
        value,
    ])
}

impl FormatNode for ClientField {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        llm_field(f, node.kw, node.colon, f.alt(f.tree.present(node.value)))
    }
}

impl FormatNode for PromptField {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        llm_field(f, node.kw, node.colon, f.alt(f.tree.present(node.value)))
    }
}

impl FormatNode for ToolsField {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        llm_field(f, node.kw, node.colon, f.alt(f.tree.present(node.value)))
    }
}

// ---------------------------------------------------------------------------
// Classes, enums, interfaces, implementations
// ---------------------------------------------------------------------------

impl FormatNode for ClassDef {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.node(f.tree.present(node.name)),
            node.generics.map_or(Doc::NIL, |generics| f.node(generics)),
            Doc::SPACE,
            f.body(
                AnyNodeId::of(id),
                f.tree.present(node.l),
                f.tree.list(node.members),
                f.tree.present(node.r),
            ),
        ])
    }
}

/// The attributes after a member's name or type: on its line when they fit,
/// else each on a line of its own, indented.
fn attributes<'a>(f: &Formatter<'a>, attrs: List<Id<Attribute>>) -> Doc<'a> {
    let d = f.docs;
    d.indent(
        d.concat(
            f.tree
                .list(attrs)
                .iter()
                .map(|&attr| d.concat([Doc::SOFT_LINE_OR_SPACE, f.node(attr)])),
        ),
    )
}

/// `name: T @attrs,` — the colon and the comma are canonical.
impl FormatNode for Field {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let name = f.tree.get(node.name).token.idx;
        d.concat([
            f.docs(node.docs, name),
            d.group(d.concat([
                f.node(node.name),
                f.separator(node.colon, ":"),
                Doc::SPACE,
                f.alt(f.tree.present(node.ty)),
                attributes(f, node.attrs),
            ])),
            f.separator(node.sep, ","),
        ])
    }
}

impl FormatNode for EnumDef {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.node(f.tree.present(node.name)),
            Doc::SPACE,
            f.body(
                AnyNodeId::of(id),
                f.tree.present(node.l),
                f.tree.list(node.members),
                f.tree.present(node.r),
            ),
        ])
    }
}

/// `Name @attrs,`
impl FormatNode for EnumVariant {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let name = f.tree.get(node.name).token.idx;
        d.concat([
            f.docs(node.docs, name),
            d.group(d.concat([f.node(node.name), attributes(f, node.attrs)])),
            f.separator(node.sep, ","),
        ])
    }
}

/// `interface Name<G> requires A, B { … }`. The required interfaces break
/// one per line under the `requires` when they do not fit, and the `{` then
/// takes a line of its own, as rustfmt lays out a trait's supertraits.
impl FormatNode for InterfaceDef {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let (requires, group) = match node.requires {
            Some(requires) => {
                let (requires, group) = requires_clause(f, requires);
                (d.concat([Doc::SPACE, requires]), Some(group))
            }
            None => (Doc::NIL, None),
        };
        d.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.node(f.tree.present(node.name)),
            node.generics.map_or(Doc::NIL, |generics| f.node(generics)),
            requires,
            f.brace_after(group),
            f.body(
                AnyNodeId::of(id),
                f.tree.present(node.l),
                f.tree.list(node.members),
                f.tree.present(node.r),
            ),
        ])
    }
}

/// `requires A, B`, the clause's comments around it, and the group of its
/// items (see [`Formatter::separated`]). After a comment that ends the line
/// before it, the keyword starts the next one level in.
fn requires_clause<'a>(f: &Formatter<'a>, id: Id<RequiresClause>) -> (Doc<'a>, GroupId<'a>) {
    let node = f.tree.get(id);
    let types = f.tree.present(node.types);
    let (items, group) = f.separated(f.tree.list(types.as_list()), |ty| f.alt(ty));
    let doc = f.docs.concat([f.docs.indent(f.token(node.kw)), items]);
    (f.with_comments(id, doc), group)
}

impl FormatNode for RequiresClause {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        requires_clause(f, id).0
    }
}

/// `type Name extends B = D;` declared in an interface; when it does not
/// fit, the bound and the default each take a line of their own under the
/// name.
impl FormatNode for AssocTypeDecl {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        d.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.node(f.tree.present(node.name)),
            d.group(d.indent(d.concat([
                node.bound.map_or(Doc::NIL, |bound| {
                    d.concat([
                        Doc::SOFT_LINE_OR_SPACE,
                        f.token(bound.kw),
                        Doc::SPACE,
                        f.alt(f.tree.present(bound.ty)),
                    ])
                }),
                node.default.map_or(Doc::NIL, |default| {
                    d.concat([
                        Doc::SOFT_LINE_OR_SPACE,
                        f.token(default.eq),
                        Doc::SPACE,
                        f.alt(f.tree.present(default.ty)),
                    ])
                }),
            ]))),
            f.separator(node.sep, ";"),
        ])
    }
}

/// `type Name = T;` bound in an implementation.
impl FormatNode for AssocTypeDef {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.node(f.tree.present(node.name)),
            Doc::SPACE,
            f.token(f.tree.present(node.eq)),
            Doc::SPACE,
            f.alt(f.tree.present(node.ty)),
            f.separator(node.sep, ";"),
        ])
    }
}

/// `implements I { … }`; `implement` written in its place is printed as
/// `implements`.
impl FormatNode for ImplementsBlock {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        d.concat([
            f.docs(node.docs, node.kw.idx),
            d.text("implements"),
            f.comments_after(node.kw.idx),
            Doc::SPACE,
            f.alt(f.tree.present(node.target)),
            Doc::SPACE,
            f.body(
                AnyNodeId::of(id),
                f.tree.present(node.l),
                f.tree.list(node.members),
                f.tree.present(node.r),
            ),
        ])
    }
}

/// `implement<G> Target for Type { … }`; `implements` written in its place
/// is printed as `implement`, the spelling of a free-standing
/// implementation (the block inside a class keeps `implements`). When the
/// header does not fit, or a line comment after the target ends its line,
/// `for Type` moves to a line of its own under it and the `{` then takes
/// a line of its own, as rustfmt lays out an `impl` header.
impl FormatNode for ImplementsFor {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let target = f.tree.present(node.target).node_id();
        // The target's trailing comments are inside the group, so that a
        // line comment there breaks it.
        let (for_clause, group) = d.group_with_id(d.concat([
            f.trailing_of(target),
            d.indent(d.concat([
                Doc::SOFT_LINE_OR_SPACE,
                f.token(f.tree.present(node.for_kw)),
                Doc::SPACE,
                f.alt(f.tree.present(node.self_ty)),
            ])),
        ]));
        d.concat([
            f.docs(node.docs, node.kw.idx),
            d.text("implement"),
            f.comments_after(node.kw.idx),
            node.generics.map_or(Doc::NIL, |generics| f.node(generics)),
            Doc::SPACE,
            f.leading_of(target),
            f.bare(target),
            for_clause,
            f.brace_after(Some(group)),
            f.body(
                AnyNodeId::of(id),
                f.tree.present(node.l),
                f.tree.list(node.members),
                f.tree.present(node.r),
            ),
        ])
    }
}

/// `interface_field as class_field,`
impl FormatNode for InterfaceFieldLink {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let first = f.tree.get(node.iface_field).token.idx;
        f.docs.concat([
            f.docs(node.docs, first),
            f.node(node.iface_field),
            Doc::SPACE,
            f.token(f.tree.present(node.as_kw)),
            Doc::SPACE,
            f.node(f.tree.present(node.class_field)),
            f.separator(node.sep, ","),
        ])
    }
}

// ---------------------------------------------------------------------------
// Aliases, clients, tests, globals
// ---------------------------------------------------------------------------

impl FormatNode for TypeAliasDef {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.node(f.tree.present(node.name)),
            Doc::SPACE,
            f.token(f.tree.present(node.eq)),
            Doc::SPACE,
            f.alt(f.tree.present(node.ty)),
            f.separator(node.semi, ";"),
        ])
    }
}

/// `client Name = value;`
impl FormatNode for ClientValueDef {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.node(node.name),
            Doc::SPACE,
            f.token(node.eq),
            Doc::SPACE,
            f.alt(f.tree.present(node.value)),
            f.separator(node.semi, ";"),
        ])
    }
}

/// `test "name" with a, b { … }`
/// `test "name" with a, b { … }`: the `with` items break one per line
/// under the keyword when they do not fit, and the `{` then takes a line of
/// its own (see [`Formatter::separated`]).
impl FormatNode for TestDef {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let (with, group) = match node.with {
            Some(with) => {
                let (with, group) = with_clause(f, with);
                (d.concat([Doc::SPACE, with]), Some(group))
            }
            None => (Doc::NIL, None),
        };
        d.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.alt(f.tree.present(node.name)),
            with,
            f.brace_after(group),
            f.node(f.tree.present(node.body)),
        ])
    }
}

/// `with a, b`, the clause's comments around it, and the group of its items
/// (see [`Formatter::separated`]). After a comment that ends the line before
/// it, the keyword starts the next one level in.
pub(super) fn with_clause<'a>(f: &Formatter<'a>, id: Id<WithClause>) -> (Doc<'a>, GroupId<'a>) {
    let node = f.tree.get(id);
    let (items, group) = f.separated(f.tree.list(node.items), |item| f.alt(item));
    let doc = f.docs.concat([f.docs.indent(f.token(node.kw)), items]);
    (f.with_comments(id, doc), group)
}

impl FormatNode for WithClause {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        with_clause(f, id).0
    }
}

/// A documented file-scope `let`.
impl FormatNode for LetItem {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let first = f.tree.span(node.stmt).start();
        f.docs.concat([f.docs(node.docs, first), f.node(node.stmt)])
    }
}
