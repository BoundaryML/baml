//! Every node of the BAML grammar.
//!
//! Field order is source order. A field's type is its grammar position:
//! `Token<…>`/`Id<…>` present, `Result<_, Missing>` required but possibly
//! missing after an error, `Option<_>` optional, `List<_>`/`Sep<_, _>`
//! repeated. `Any*` enums are the alternatives at a position; each has a
//! `Bogus` variant for recovered garbage.
//!
//! Trivia (whitespace, comments) never appear in a node; they stay in the
//! token array between the tokens nodes own. The one exception is a `//#`
//! header line in a block, which is a statement ([`HeaderStmt`]).

mod exprs;
mod items;
mod patterns;
mod stmts;
mod templates;
mod types;

pub use exprs::*;
pub use items::*;
pub use patterns::*;
pub use stmts::*;
pub use templates::*;
pub use types::*;

use crate::{
    token::punct,
    tree::{Bogus, Id, SepItem},
};

crate::define_tree! {
    root: SourceFile,
    nodes {
        // items
        SourceFile, FunctionDef, ReturnType, ThrowsClause, LlmBody, ClientField, PromptField, ToolsField,
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
        ExprStmt, LetStmt, TypeBindingStmt, ReturnStmt, ThrowStmt, BreakStmt, ContinueStmt, DeferStmt,
        WhileStmt, WhileLetStmt, ForStmt, ForInHeader, CForHeader, HeaderStmt,
        // templates
        BacktickString, TemplateInterp, TemplateFor, TemplateForOpen, TemplateIf, TemplateIfOpen,
        TemplateElif, TemplateElse, TemplateEndTag,
        // recovery
        Bogus,
    },
    pools {
        // separator-led arms and segments
        UnionArm, PatternArm, BoundArm,
        AnyItem, AnyLlmField, AnyClassMember, AnyEnumMember, AnyInterfaceMember, AnyImplMember,
        AnyStmt, AnyObjectMember, AnyTemplatePart,
        Id<BlockAttr>, Id<Attribute>, Id<CatchClause>, Id<CatchArm>, Id<MatchArm>, Id<MapEntry>,
        Id<TemplateElif>,
        PathSegment, ArrayElement,
        SepItem<AnyType, punct::Comma>,
        SepItem<AnyTypeArg, punct::Comma>,
        SepItem<Id<GenericParam>, punct::Comma>,
        SepItem<Id<Param>, punct::Comma>,
        SepItem<AnyAttrArg, punct::Comma>,
        SepItem<Id<FunctionTypeParam>, punct::Comma>,
        SepItem<Id<FieldPattern>, punct::Comma>,
        SepItem<AnyArrayPatternElement, punct::Comma>,
        SepItem<Id<Arg>, punct::Comma>,
        SepItem<AnyExpr, punct::Comma>,
    },
}
