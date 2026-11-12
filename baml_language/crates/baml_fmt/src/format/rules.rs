//! The rule for each node type. A node type listed under [`verbatim`] has no
//! layout of its own yet and prints as written; as rules are written, their
//! types move out of that list (a type in both places, or in neither, does
//! not compile).

use baml_syntax::{
    nodes::{self, AnyItem, Bogus, SourceFile},
    tree::{Id, NodeUnion as _},
};

use super::{FormatNode, Formatter};
use crate::{comments::docs, ir::Doc};

/// Items are separated by one blank line, except that a run of globals keeps
/// the grouping it was written with: adjacent `let`s stay adjacent, and a
/// blank line the author put between two of them stays. Comments before the
/// first item come first; a file with nothing but comments is those
/// comments.
impl FormatNode for SourceFile {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        let d = f.docs;
        let items = f.tree.list(f.tree.get(id).items);
        let mut parts = vec![docs::dangling(
            d,
            f.tree,
            f.comments.dangling(f.tree.root_id()),
        )];
        for (index, &item) in items.iter().enumerate() {
            if index > 0 {
                let grouped_globals = matches!(item, AnyItem::Let(_))
                    && matches!(items[index - 1], AnyItem::Let(_))
                    && !f
                        .comments
                        .gap_before_node(f.tree, item.node_id())
                        .has_blank_line();
                parts.push(if grouped_globals {
                    Doc::HARD_LINE
                } else {
                    Doc::EMPTY_LINE
                });
            }
            parts.push(f.alt(item));
        }
        parts.push(Doc::HARD_LINE);
        d.concat(parts)
    }
}

impl FormatNode for Bogus {
    fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
        f.tree.absent(id)
    }
}

/// The node types that print as written until their rule exists.
macro_rules! verbatim {
    ($($node:ident),* $(,)?) => {
        $(impl FormatNode for nodes::$node {
            fn fmt<'a>(id: Id<Self>, f: &Formatter<'a>) -> Doc<'a> {
                f.verbatim(id)
            }
        })*
    };
}

verbatim! {
    // items
    FunctionDef, ReturnType, ThrowsClause, LlmBody, ClientField, PromptField, ToolsField,
    ClassDef, Field, EnumDef, EnumVariant, InterfaceDef, RequiresClause, MethodSig, AssocTypeDecl,
    AssocTypeDef,
    ImplementsBlock, ImplementsFor, InterfaceFieldLink, TypeAliasDef, ClientValueDef, TestDef,
    WithClause, RemovedItem,
    GenericParamList, GenericParam, GenericParamBounds, GenericArgs, TypeArgs, AssocTypeBinding,
    ParamList, Param, TypeAscription, DefaultValue,
    Attribute, BlockAttr, DottedName, AttrArgs, JinjaArg,
    // types
    PathType, LiteralType, ParenType, FunctionType, FunctionTypeParam, OptionalType, ArrayType,
    UnionType, QualifiedPath,
    // patterns
    BindingPattern, WildcardPattern, DestructurePattern, FieldPattern, ArrayPattern, RestPattern,
    TypePattern, ParenPattern, UnionPattern,
    // expressions
    Name, PathExpr, GenericApplyExpr, LiteralExpr, StringLit, FieldAccessExpr, OptionalFieldAccessExpr,
    UpcastExpr, SpecExpr, EnvAccessExpr, CallExpr, ArgList, Arg, OptionalCallExpr, IndexExpr,
    OptionalIndexExpr, TaggedTemplateExpr, BinaryExpr, IsExpr, UnaryExpr, AwaitExpr, SpawnExpr,
    CatchExpr, CatchClause, CatchArm, ThrowExpr, ReturnExpr, BreakExpr, ContinueExpr, ParenExpr,
    Block, IfExpr, IfLetExpr, MatchExpr, MatchParen, MatchArm, LambdaExpr, ArrayLiteral, MapLiteral,
    MapEntry, ObjectLiteral, ObjectField, SpreadElement,
    // statements
    ExprStmt, AssignStmt, EmptyStmt, LetStmt, TypeBindingStmt, DeferStmt, WhileStmt, WhileLetStmt, ForStmt,
    ForInHeader, ParenForInHeader, CForHeader, HeaderStmt, DocLine, Directive, HeaderLine, LetItem,
    // templates
    BacktickString, TemplateInterp, TemplateFor, TemplateForOpen, TemplateIf, TemplateIfOpen,
    TemplateElif, TemplateElse, TemplateEndTag,
    // tuples (canary #5030), laid out by the rules of their families
    TupleExpr, TupleFieldExpr, TuplePattern, TupleType,
}
