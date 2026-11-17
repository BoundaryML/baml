//! Declarations and their parts: functions and method signatures, classes,
//! enums, interfaces, implementations, aliases, tests, globals, and the
//! parameter lists, docs and LLM bodies they own. One member per line in a
//! body, `name: T,` for a field, `;` after a signature or associated type,
//! parameters one per line with a trailing comma when the signature breaks.

use baml_syntax::{
    Token,
    nodes::{
        AnyDoc, AnyFunctionBody, AnyLlmField, AnyType, AssocTypeDecl, AssocTypeDef, Attribute,
        ClassDef, ClientField, ClientValueDef, DefaultValue, Directive, DocLine, EnumDef,
        EnumVariant, Field, FunctionDef, GenericParamList, HeaderLine, ImplementsBlock,
        ImplementsFor, InterfaceDef, InterfaceFieldLink, LetItem, LlmBody, MethodSig, Name, Param,
        ParamList, PromptField, RequiresClause, ReturnType, TestDef, ThrowsClause, ToolsField,
        TypeAliasDef, TypeAscription, WithClause,
    },
    token::{Token, TokenIdx, TokenSet},
    tree::{AnyNodeId, Id, List, Missing, NodeUnion},
};

use crate::{
    format::{FormatNode, Formatter, layout::multi_line_before_last},
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

/// A `///`, `//baml:` or `//#` line, without the trailing blanks it was
/// written with (an editor continuing a doc comment leaves `/// `).
pub(super) fn comment_line<'a, S: TokenSet>(f: &Formatter<'a>, token: Token<S>) -> Doc<'a> {
    f.spelled(f.tree.token_text(token.idx).trim_end(), token.idx)
}

impl FormatNode for DocLine {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        comment_line(f, f.tree.get(id).token)
    }
}

impl FormatNode for Directive {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        comment_line(f, f.tree.get(id).token)
    }
}

impl FormatNode for HeaderLine {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        comment_line(f, f.tree.get(id).token)
    }
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// The signature a function definition and a method signature share.
struct Signature {
    docs: List<AnyDoc>,
    kw: Token!["function"],
    name: Id<Name>,
    generics: Option<Id<GenericParamList>>,
    params: Id<ParamList>,
    ret: Id<ReturnType>,
    throws: Option<Id<ThrowsClause>>,
}

impl Signature {
    /// `function name<G>(params) -> R throws E`, the docs before it (see
    /// [`signature`] for the layout).
    fn doc<'a>(&self, f: &Formatter<'a>) -> Doc<'a> {
        let d = f.docs;
        let ret = AnyNodeId::of(self.ret);
        let params = AnyNodeId::of(self.params);
        let (rest, _) = signature(
            f,
            d.concat([f.leading_of(params), f.bare(params)]),
            f.trailing_of(params),
            d.concat([f.leading_of(ret), f.bare(ret)]),
            f.trailing_of(ret),
            self.throws,
        );
        d.concat([
            f.docs(self.docs, self.kw.idx),
            f.token(self.kw),
            Doc::SPACE,
            f.node(self.name),
            self.generics.map_or(Doc::NIL, |generics| f.node(generics)),
            rest,
        ])
    }
}

/// `(params) -> R throws E` after a name or before a lambda's body, laid
/// out as rustfmt lays a signature with a `where` clause out: all on one
/// line; else `throws E` on a line of its own under the return type — the
/// clause moves before the parameters break, as a `where` clause never
/// breaks them; else the parameters one per line, `) -> R` on the closing
/// line and `throws E` beside it when it then fits. The parameters and the
/// return type are one group, so the parameters break before a return type
/// does; the `throws` clause is a group of its own. The whole is a named
/// group, returned so that a lambda can break its body once its signature
/// broke. A line comment after the parameters ends the line, and what
/// follows it goes one level in; one after or inside the return type leaves
/// the parameters to themselves. What follows the signature — a body's `{`
/// — goes after a [`Doc::HANG_LINE`]: on the signature's last line when
/// that line is the signature's own, on a line of its own when it is a
/// continuation line (`    throws E`, `    | B`).
pub(super) fn signature<'a>(
    f: &Formatter<'a>,
    params: Doc<'a>,
    params_trailing: Doc<'a>,
    ret: Doc<'a>,
    ret_trailing: Doc<'a>,
    throws: Option<Id<ThrowsClause>>,
) -> (Doc<'a>, GroupId<'a>) {
    let d = f.docs;
    let throws = match throws {
        None => Doc::NIL,
        Some(throws) => {
            let id = AnyNodeId::of(throws);
            d.concat([
                d.group(d.indent(d.concat([
                    Doc::SOFT_LINE_OR_SPACE,
                    f.leading_of(id),
                    f.bare(id),
                ]))),
                f.trailing_of(id),
            ])
        }
    };
    let ret = d.concat([Doc::SPACE, ret]);
    let doc = if params_trailing.will_break() {
        d.concat([
            d.group(params),
            params_trailing,
            d.indent(d.concat([ret, ret_trailing, throws])),
        ])
    } else if ret.will_break() {
        // A comment inside the return type ends a line: grouped with it,
        // the parameters would break for nothing.
        d.concat([
            d.group(d.concat([params, params_trailing])),
            ret,
            ret_trailing,
            throws,
        ])
    } else {
        d.concat([
            d.group(d.concat([params, params_trailing, ret])),
            ret_trailing,
            throws,
        ])
    };
    d.group_with_id(doc)
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
            AnyFunctionBody::Expr(block) => f.expanded_block(block),
            AnyFunctionBody::Llm(llm) => f.node(llm),
            AnyFunctionBody::Bogus(bogus) => f.tree.absent(bogus),
        };
        f.docs.concat([signature.doc(f), Doc::HANG_LINE, body])
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
        f.docs
            .concat([signature.doc(f), f.separator(node.semi, ";")])
    }
}

/// `-> T`; a `=>` written in its place is printed as `->`.
impl FormatNode for ReturnType {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.spelled("->", node.arrow.idx),
            f.after_operator([node.arrow.idx], f.tree.present(node.ty)),
        ])
    }
}

impl FormatNode for ThrowsClause {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.token(node.kw),
            f.after_operator([node.kw.idx], f.tree.present(node.ty)),
        ])
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
            None,
            AnyNodeId::of,
            |param| f.node(param),
        )
    }
}

/// `name: T = default`; `self` has neither. After a comment that ends the
/// name's or the type's line, what follows starts the next one level in.
impl FormatNode for Param {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let name = f.tree.present(node.name);
        let before_default = node.ascription.map_or(AnyNodeId::of(name), AnyNodeId::of);
        let default = node.default.map_or(Doc::NIL, |default| {
            f.continuation(before_default, f.node(default))
        });
        f.docs.concat([
            f.node(name),
            f.continuation(
                AnyNodeId::of(name),
                f.docs.concat([
                    node.ascription
                        .map_or(Doc::NIL, |ascription| f.node(ascription)),
                    default,
                ]),
            ),
        ])
    }
}

/// `: T`, the colon added where it was left out.
impl FormatNode for TypeAscription {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            f.punct(node.colon, ":"),
            f.after_operator(node.colon.map(|colon| colon.idx), f.tree.present(node.ty)),
        ])
    }
}

/// ` = value`, laid out as an initialiser: the value moves to a line of its
/// own before the type before it breaks.
impl FormatNode for DefaultValue {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        f.docs.concat([
            Doc::SPACE,
            f.token(node.eq),
            f.assigned([node.eq.idx], f.tree.present(node.value)),
        ])
    }
}

// ---------------------------------------------------------------------------
// LLM bodies
// ---------------------------------------------------------------------------

/// `{ client: … tools: … prompt: … }`, the fields in that order however
/// they were written. Comments on lines of their own before the `}` trail
/// the field written last; they stay at the end whichever field that is.
impl FormatNode for LlmBody {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let written = f.tree.list(node.fields);
        let mut fields = written.to_vec();
        fields.sort_by_key(|field| match field {
            AnyLlmField::Client(_) => 0,
            AnyLlmField::Tools(_) => 1,
            AnyLlmField::Prompt(_) => 2,
            AnyLlmField::Bogus(bogus) => f.tree.absent(*bogus),
        });
        let last_written = written.last().map(NodeUnion::node_id);
        let mut items: Vec<(AnyNodeId, Doc<'a>)> = fields
            .iter()
            .map(|field| {
                let id = field.node_id();
                let doc = if Some(id) == last_written {
                    let (on_line, _) = f.trailing_split(id);
                    d.concat([f.leading_of(id), f.bare(id), on_line])
                } else {
                    f.any(id)
                };
                (id, doc)
            })
            .collect();
        if let (Some(last_written), Some(last)) = (last_written, items.last_mut()) {
            let (_, closing) = f.trailing_split(last_written);
            last.1 = d.concat([last.1, closing]);
        }
        f.braced_items(
            AnyNodeId::of(id),
            node.l,
            &items,
            f.tree.present(node.r),
            false,
        )
    }
}

/// `key: value` of an LLM body.
fn llm_field<'a, K: TokenSet>(
    f: &Formatter<'a>,
    kw: Token<K>,
    colon: Result<Token![":"], Missing>,
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
/// else each on a line of its own, indented. One with a multi-line template
/// in it is measured by its first line, so a `@description(…)` holding one
/// stays on the member's line — unless another attribute follows it, which
/// would land on the template's closing line: then each takes a line of its
/// own.
fn attributes<'a>(f: &Formatter<'a>, attrs: List<Id<Attribute>>) -> Doc<'a> {
    let d = f.docs;
    let docs: Vec<Doc<'a>> = f
        .tree
        .list(attrs)
        .iter()
        .map(|&attr| f.node(attr))
        .collect();
    d.concat([
        multi_line_before_last(&docs),
        d.indent(
            d.concat(
                docs.iter()
                    .map(|&attr| d.concat([Doc::SOFT_LINE_OR_SPACE, attr])),
            ),
        ),
    ])
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
                f.continuation(
                    AnyNodeId::of(node.name),
                    d.concat([
                        f.punct(node.colon, ":"),
                        f.after_operator(
                            node.colon.map(|colon| colon.idx),
                            f.tree.present(node.ty),
                        ),
                        attributes(f, node.attrs),
                    ]),
                ),
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
/// takes a line of its own ([`Doc::HANG_LINE`]), as rustfmt lays out a
/// trait's supertraits.
impl FormatNode for InterfaceDef {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let name = f.tree.present(node.name);
        // What the clause follows: a line comment there starts it on the
        // next line, one level in, its items one level further.
        let before = node.generics.map_or(AnyNodeId::of(name), AnyNodeId::of);
        let requires = match node.requires {
            Some(requires) => {
                f.continuation(before, d.concat([Doc::SPACE, requires_clause(f, requires)]))
            }
            None => Doc::NIL,
        };
        d.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.node(name),
            node.generics.map_or(Doc::NIL, |generics| f.node(generics)),
            requires,
            Doc::HANG_LINE,
            f.body(
                AnyNodeId::of(id),
                f.tree.present(node.l),
                f.tree.list(node.members),
                f.tree.present(node.r),
            ),
        ])
    }
}

/// `requires A, B` (see [`Formatter::separated`]). After a comment that
/// ends the line before it, the keyword starts the next one level in.
fn requires_clause<'a>(f: &Formatter<'a>, id: Id<RequiresClause>) -> Doc<'a> {
    let node = f.tree.get(id);
    let types = f.tree.present(node.types);
    let any = AnyNodeId::of(id);
    let (items, _) = f.separated(f.tree.list(types.as_list()), f.trailing_of(any), |ty| {
        f.alt(ty)
    });
    f.docs.concat([f.leading_of(any), f.token(node.kw), items])
}

/// A `requires` clause with its comments: the leading ones before it, the
/// trailing ones with its last item (see [`Formatter::separated`]).
impl FormatNode for RequiresClause {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        requires_clause(f, id)
    }
}

/// `type Name extends B = D;` declared in an interface; when it does not
/// fit, the bound and the default each take a line of their own under the
/// name. With comments between a keyword and its type, the clauses stay on
/// the name's line and the type hangs under its keyword, as after
/// `type X = // c` in an implementation.
impl FormatNode for AssocTypeDecl {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let joints: Vec<(TokenIdx, AnyType)> = node
            .bound
            .map(|bound| (bound.kw.idx, f.tree.present(bound.ty)))
            .into_iter()
            .chain(
                node.default
                    .map(|default| (default.eq.idx, f.tree.present(default.ty))),
            )
            .collect();
        let commented = joints
            .iter()
            .any(|&(kw, ty)| f.comments_between([kw], ty.node_id()));
        let joint = |&(kw, ty): &(TokenIdx, AnyType)| {
            let line = if commented {
                Doc::SPACE
            } else {
                Doc::SOFT_LINE_OR_SPACE
            };
            d.concat([line, f.token_at(kw), f.after_operator([kw], ty)])
        };
        let clauses = d.concat(joints.iter().map(joint));
        let clauses = if commented {
            clauses
        } else {
            d.group(d.indent(clauses))
        };
        d.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.node(f.tree.present(node.name)),
            clauses,
            f.separator(node.sep, ";"),
        ])
    }
}

/// `type Name = T;` bound in an implementation.
impl FormatNode for AssocTypeDef {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let name = f.tree.present(node.name);
        let eq = f.tree.present(node.eq);
        f.docs.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.node(name),
            f.continuation(
                AnyNodeId::of(name),
                f.docs.concat([
                    Doc::SPACE,
                    f.token(eq),
                    f.after_operator([eq.idx], f.tree.present(node.ty)),
                ]),
            ),
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
            f.spelled("implements", node.kw.idx),
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
        // line comment there breaks it; the implemented-for type's go after
        // it, so that a line comment there ends the header's line without
        // moving `for` under the target.
        let self_ty = f.tree.present(node.self_ty).node_id();
        let for_clause = d.group(d.concat([
            f.trailing_of(target),
            d.indent(d.concat([
                Doc::SOFT_LINE_OR_SPACE,
                f.token(f.tree.present(node.for_kw)),
                Doc::SPACE,
                f.leading_of(self_ty),
                f.bare(self_ty),
            ])),
        ]));
        let for_clause = d.concat([for_clause, f.trailing_of(self_ty)]);
        d.concat([
            f.docs(node.docs, node.kw.idx),
            f.spelled("implement", node.kw.idx),
            node.generics.map_or(Doc::NIL, |generics| f.node(generics)),
            Doc::SPACE,
            f.leading_of(target),
            f.bare(target),
            for_clause,
            Doc::HANG_LINE,
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
            f.continuation(
                AnyNodeId::of(node.iface_field),
                f.docs.concat([
                    Doc::SPACE,
                    f.token(f.tree.present(node.as_kw)),
                    Doc::SPACE,
                    f.node(f.tree.present(node.class_field)),
                ]),
            ),
            f.separator(node.sep, ","),
        ])
    }
}

// ---------------------------------------------------------------------------
// Aliases, clients, tests, globals
// ---------------------------------------------------------------------------

/// `type Name = T;` — the type stays on the `=` line and breaks at its own
/// points (a union one `| member` per line), never moving whole to the
/// next line as an expression may.
impl FormatNode for TypeAliasDef {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let name = f.tree.present(node.name);
        let eq = f.tree.present(node.eq);
        f.docs.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.node(name),
            f.continuation(
                AnyNodeId::of(name),
                f.docs.concat([
                    Doc::SPACE,
                    f.token(eq),
                    f.after_operator([eq.idx], f.tree.present(node.ty)),
                ]),
            ),
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
            f.continuation(
                AnyNodeId::of(node.name),
                f.docs.concat([
                    Doc::SPACE,
                    f.token(node.eq),
                    f.assigned([node.eq.idx], f.tree.present(node.value)),
                ]),
            ),
            f.separator(node.semi, ";"),
        ])
    }
}

/// `test "name" with a, b { … }`: the `with` items break one per line
/// under the keyword when they do not fit, and the `{` then takes a line of
/// its own (see [`Formatter::separated`] and [`Doc::HANG_LINE`]).
impl FormatNode for TestDef {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let node = f.tree.get(id);
        let d = f.docs;
        let name = f.tree.present(node.name);
        let with = match node.with {
            Some(with) => {
                let (clause, _) = with_clause(f, with);
                f.continuation(name.node_id(), d.concat([Doc::SPACE, clause]))
            }
            None => Doc::NIL,
        };
        d.concat([
            f.docs(node.docs, node.kw.idx),
            f.token(node.kw),
            Doc::SPACE,
            f.alt(name),
            with,
            Doc::HANG_LINE,
            f.expanded_block(f.tree.present(node.body)),
        ])
    }
}

/// `with a, b` with its comments and the group of its items (see
/// [`Formatter::separated`]), which a `spawn` reads to break its body once
/// the items broke.
pub(super) fn with_clause<'a>(f: &Formatter<'a>, id: Id<WithClause>) -> (Doc<'a>, GroupId<'a>) {
    let node = f.tree.get(id);
    let any = AnyNodeId::of(id);
    let (items, group) = f.separated(f.tree.list(node.items), f.trailing_of(any), |item| {
        f.alt(item)
    });
    (
        f.docs.concat([f.leading_of(any), f.token(node.kw), items]),
        group,
    )
}

/// A `with` clause with its comments: the leading ones before it, the
/// trailing ones with its last item (see [`Formatter::separated`]).
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
