//! Items: a `SourceFile` → the `Item`s the rowan lowering (`lower_cst`)
//! produces, read off typed fields. The desugarings — LLM companions, the
//! `cleanup` guard, the `implements … for` fold, the `$init_test` registrar —
//! are shared with the rowan path and run on the lowered declarations; only
//! what reads a tree lives here.
//!
//! Spans are the rowan lowering's: an item starts at its keyword (its `///`
//! lines and directives, trivia to rowan, come before that), and a member
//! ends before the `,`/`;` it owns. Members lower in source order, each as
//! it is met (the rowan lowering went kind by kind, which only the order of
//! diagnostics and environment references saw).

use std::path::Path;

use baml_base::Name;
use baml_syntax::{
    nodes::{
        AnyAttrArg, AnyClassMember, AnyDoc, AnyEnumMember, AnyExpr, AnyFunctionBody, AnyImplMember,
        AnyInterfaceMember, AnyItem, AnyLlmField, AnyPattern, AnyStringLike, AnyTemplatePart,
        AnyType, AssocTypeDecl, AssocTypeDef, AttrArgs, Attribute, BacktickString, Block,
        BlockAttr, ClassDef, ClientValueDef, DirectiveKind, EnumDef, EnumVariant, Field, FileTree,
        FunctionDef, GenericParamList, ImplementsBlock, ImplementsFor, InterfaceDef,
        InterfaceFieldLink, LetItem, LlmBody, MethodSig, RemovedItem, StringLit, TestDef,
        TypeAliasDef, WithClause,
    },
    token::{TokenIdx, TokenKind},
    tree::{Id, List, Missing},
};
use text_size::TextRange;

use super::{View, body, templates, types};
use crate::{
    EnvVarRef, LoweringDiagnostic,
    ast::{
        self, AssociatedTypeBindingDef, AssociatedTypeDef, BuiltinKind, DeclarativeMeta, Expr,
        ExprId, FieldDef, FunctionBodyDef, FunctionDefaults, FunctionMetadata, FunctionOrigin,
        GenericParam, ImplOrigin, ImplementsForDef, Item, LetDef, LetOrigin, LlmBodyDef,
        LlmPromptSpans, MethodSigDef, Param, Pattern, RawAttribute, RawAttributeArg,
        TemplateSegment, TraceHookDirective, TypeExpr, TypeExprKind, VariantDef,
    },
    cleanup_guard::maybe_inject_cleanup_guard,
    companions::expand_companions,
    lower_cst::{
        ClassMethodContext, TestRegistration, append_spec_client_param, append_spec_on_event_param,
        check_missing_type, classify_client_string, reject_reserved_llm_params,
        reject_unquoted_client_shorthand, synthesize_init_test_function,
        warn_quoted_prompt_interpolation,
    },
    lower_expr_body::{
        InitTestContext, LlmClient, LlmSpecSource, LoweringContext, TopLevelLetSurface,
        ascription_type_expr, strip_string_delimiters, synthesize_llm_spec_body,
        synthesize_spec_agent_run_body,
    },
    lower_type_expr::{check_throws_wildcard, check_wildcard_type},
};

/// Lower a source file's items. `file_path` names the `$init_test_<path>`
/// registrar and, with `test_owner` absent, the tests' owner.
pub fn lower_file(
    tree: &FileTree,
    file_path: Option<&Path>,
    test_owner: Option<&str>,
) -> (Vec<Item>, Vec<LoweringDiagnostic>, Vec<EnvVarRef>) {
    lower_file_on(tree, file_path, test_owner, TopLevelLetSurface::Source)
}

/// [`lower_file`] for a `Session.eval` submission, whose file-scope bindings
/// are the session's persistent ones.
pub fn lower_session_file(
    tree: &FileTree,
    file_path: Option<&Path>,
    test_owner: Option<&str>,
) -> (Vec<Item>, Vec<LoweringDiagnostic>, Vec<EnvVarRef>) {
    lower_file_on(tree, file_path, test_owner, TopLevelLetSurface::Session)
}

fn lower_file_on(
    tree: &FileTree,
    file_path: Option<&Path>,
    test_owner: Option<&str>,
    surface: TopLevelLetSurface,
) -> (Vec<Item>, Vec<LoweringDiagnostic>, Vec<EnvVarRef>) {
    let v = View::new(tree);
    let mut items = Items {
        v,
        surface,
        diags: Vec::new(),
        env_var_refs: Vec::new(),
    };
    let mut out = Vec::new();
    let mut registrations = Vec::new();
    for &item in v.tree.list(v.node(v.tree.root()).items) {
        match item {
            AnyItem::Function(id) => {
                if let Some(func) = items.function(id, None) {
                    let companions = expand_companions(&func, None, &[]);
                    out.push(Item::Function(func));
                    out.extend(companions.into_iter().map(Item::Function));
                }
            }
            AnyItem::Class(id) => {
                if let Some((class, impls)) = items.class(id) {
                    out.push(Item::Class(class));
                    out.extend(impls.into_iter().map(Item::ImplementsFor));
                }
            }
            AnyItem::Enum(id) => out.extend(items.enum_def(id).map(Item::Enum)),
            AnyItem::Interface(id) => out.extend(items.interface(id).map(Item::Interface)),
            AnyItem::ImplementsFor(id) => {
                out.extend(items.implements_for(id).map(Item::ImplementsFor));
            }
            AnyItem::TypeAlias(id) => out.extend(items.type_alias(id).map(Item::TypeAlias)),
            AnyItem::Let(id) => out.extend(items.top_level_let(id).map(Item::Let)),
            // `let type` binds a runtime type for the rest of a block, and a
            // file has no block for it to bind in.
            AnyItem::TypeBinding(id) => items.diags.push(LoweringDiagnostic::TopLevelLetType {
                span: v.range_of(id),
            }),
            AnyItem::ClientValue(id) => out.push(Item::Let(items.client_value(id))),
            AnyItem::Test(id) => registrations.extend(items.test_registration(id)),
            AnyItem::Removed(id) => items.removed_item(id),
            AnyItem::Bogus(_) => {}
        }
    }
    if !registrations.is_empty() {
        let init_fn = synthesize_init_test_function(
            &registrations,
            file_path,
            test_owner,
            &mut items.diags,
            &mut items.env_var_refs,
        );
        out.push(Item::Function(init_fn));
    }
    (out, items.diags, items.env_var_refs)
}

/// The file being lowered and what the lowering collects.
struct Items<'t> {
    v: View<'t>,
    surface: TopLevelLetSurface,
    diags: Vec<LoweringDiagnostic>,
    env_var_refs: Vec<EnvVarRef>,
}

/// A span from `start`'s start to `end`'s end.
fn span_from(start: TextRange, end: TextRange) -> TextRange {
    TextRange::new(start.start(), end.end())
}

impl<'t> Items<'t> {
    /// The span the rowan lowering gave a member: from its first own token
    /// (`start`) to the end of its last part before the separator it owns.
    fn member_span(&self, start: TextRange, node: TextRange, sep: Option<TokenIdx>) -> TextRange {
        let end = match sep.and_then(|sep| self.v.tree.tokens().prev_significant(sep)) {
            Some(last) => self.v.token_range(last).end(),
            None => node.end(),
        };
        TextRange::new(start.start(), end)
    }

    /// A type at a declaration site: lowered, checked for `Missing` with
    /// `missing_context`, for `_` with `wildcard_context` when given, and
    /// spanning the type as written.
    fn declared_type(
        &mut self,
        ty: AnyType,
        missing_context: String,
        wildcard_context: Option<&str>,
    ) -> TypeExpr {
        let span = self.v.range_any(&ty);
        let mut expr = types::lower_type(self.v, &mut self.diags, ty);
        check_missing_type(&expr, missing_context, span, &mut self.diags);
        if let Some(context) = wildcard_context {
            check_wildcard_type(&mut expr, context, span, &mut self.diags);
        }
        expr.with_span(span)
    }

    // ── Docs, directives, attributes ─────────────────────────────────────────

    /// A function's `//#` title: the header line's text after its `#`s,
    /// trimmed; `None` when there is no header or the title is empty.
    fn header_title(&self, docs: List<AnyDoc>) -> Option<String> {
        self.v.tree.list(docs).iter().find_map(|doc| {
            let AnyDoc::Header(id) = doc else { return None };
            let text = self.v.text(self.v.node(*id).token.idx);
            let title = text
                .strip_prefix("//")
                .unwrap_or(text)
                .trim_start_matches('#')
                .trim();
            (!title.is_empty()).then(|| title.to_string())
        })
    }

    /// A declaration's docstring: its `///` lines joined with `\n`, each
    /// without the `///` and one optional leading space. A plain comment,
    /// a blank line or a `//#` title between the lines is transparent (as in
    /// Rust); a directive or a trace hook is not a line of it.
    fn docstring(&self, docs: List<AnyDoc>) -> Option<String> {
        let lines: Vec<String> = self
            .v
            .tree
            .list(docs)
            .iter()
            .filter_map(|doc| match doc {
                AnyDoc::Doc(id) => Some(self.v.node(*id).token.idx),
                AnyDoc::Directive(_) | AnyDoc::Header(_) | AnyDoc::Bogus(_) => None,
            })
            .map(|token| {
                let text = self.v.text(token);
                let doc = text.strip_prefix("///").unwrap_or(text);
                doc.strip_prefix(' ').unwrap_or(doc).to_string()
            })
            .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    /// Whether a `//baml:<kind>` directive is among the declaration's docs.
    fn has_directive(&self, docs: List<AnyDoc>, kind: DirectiveKind) -> bool {
        self.v
            .tree
            .list(docs)
            .iter()
            .any(|doc| matches!(doc, AnyDoc::Directive(id) if self.v.node(*id).kind == kind))
    }

    /// The function's `/// baml:$trace=path` hook. The parser admits one
    /// per function at most, and only with a hook path.
    fn trace_hook(&self, docs: List<AnyDoc>) -> Option<TraceHookDirective> {
        let mut hooks = self.v.tree.list(docs).iter().filter_map(|doc| match doc {
            AnyDoc::Directive(id) if self.v.node(*id).kind == DirectiveKind::Trace => Some(*id),
            AnyDoc::Directive(_) | AnyDoc::Doc(_) | AnyDoc::Header(_) | AnyDoc::Bogus(_) => None,
        });
        let directive = *self.v.node(hooks.next()?);
        let second = hooks.next();
        debug_assert!(
            second.is_none(),
            "the parser admits one trace hook per function"
        );
        let path = DirectiveKind::trace_hook_value(self.v.text(directive.token.idx))
            .and_then(DirectiveKind::trace_hook_path)
            .unwrap_or_else(|| unreachable!("a `Trace` directive carries a hook path"));
        Some(TraceHookDirective {
            path: path.split('.').map(Name::new).collect(),
            span: self.v.token_range(directive.token.idx),
        })
    }

    /// One `@@` attribute; none when its name is missing (reported by the
    /// parser).
    fn block_attribute(&self, id: Id<BlockAttr>) -> Option<RawAttribute> {
        let attr = *self.v.node(id);
        Some(self.raw_attribute(attr.path.ok()?, attr.args, self.v.range_of(id)))
    }

    /// The `@` attributes of a member.
    fn member_attributes(&self, attrs: List<Id<Attribute>>) -> Vec<RawAttribute> {
        self.v
            .tree
            .list(attrs)
            .iter()
            .filter_map(|&id| {
                let attr = *self.v.node(id);
                Some(self.raw_attribute(attr.path.ok()?, attr.args, self.v.range_of(id)))
            })
            .collect()
    }

    fn raw_attribute(
        &self,
        path: Id<baml_syntax::nodes::DottedName>,
        args: Option<Id<AttrArgs>>,
        span: TextRange,
    ) -> RawAttribute {
        let path = *self.v.node(path);
        let name = types::dotted_text(self.v, path.head, path.segments);
        // An argument is kept as its source text.
        let args = args
            .map(|args| {
                self.v
                    .tree
                    .list(self.v.node(args).args)
                    .iter()
                    .map(|arg| {
                        let span = self.v.range_any(&arg.item);
                        let text = match arg.item {
                            AnyAttrArg::Unquoted(name) => self.v.text(self.v.node(name).token.idx),
                            AnyAttrArg::String(_)
                            | AnyAttrArg::Backtick(_)
                            | AnyAttrArg::Jinja(_)
                            | AnyAttrArg::Bogus(_) => &self.v.tree.text()[span],
                        };
                        RawAttributeArg {
                            key: None,
                            value: text.trim().to_string(),
                            span,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        RawAttribute {
            name: Name::new(&name),
            args,
            span,
        }
    }

    // ── Generics, parameters ─────────────────────────────────────────────────

    /// BEP-044 generic bounds: each parameter's name with **every**
    /// `&`-separated bound it declares (`<T>` → `(T, [])`; `<T extends A & B>`
    /// → `(T, [A, B])`). The bound list is a conjunction.
    fn generic_params(&mut self, list: Option<Id<GenericParamList>>) -> Vec<GenericParam> {
        let Some(list) = list else {
            return Vec::new();
        };
        let params: Vec<_> = self
            .v
            .tree
            .list(self.v.node(list).params)
            .iter()
            .map(|item| item.item)
            .collect();
        params
            .into_iter()
            .map(|id| {
                let param = *self.v.node(id);
                let bounds = param
                    .bounds
                    .map(|bounds| {
                        let bounds = *self.v.node(bounds);
                        let arms: Vec<_> = self.v.tree.list(bounds.rest).to_vec();
                        bounds
                            .first
                            .ok()
                            .into_iter()
                            .chain(arms.into_iter().filter_map(|arm| arm.ty.ok()))
                            .map(|ty| {
                                let mut bound = types::lower_type(self.v, &mut self.diags, ty);
                                let span = bound.span;
                                check_wildcard_type(
                                    &mut bound,
                                    "a generic type bound",
                                    span,
                                    &mut self.diags,
                                );
                                bound
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                GenericParam {
                    name: self.v.name(param.name),
                    bounds,
                    name_span: self.v.name_range(param.name),
                }
            })
            .collect()
    }

    fn return_type(
        &mut self,
        ret: Result<Id<baml_syntax::nodes::ReturnType>, Missing>,
        name: &Name,
    ) -> Option<TypeExpr> {
        // An arrow with no type after it is a type that is missing, not a
        // function without a declared return type.
        Some(match self.v.node(ret.ok()?).ty {
            Ok(ty) => self.declared_type(
                ty,
                format!("return type of `{name}`"),
                Some("a return type"),
            ),
            Err(missing) => TypeExprKind::Missing.at(View::missing_range(missing)),
        })
    }

    // ── Functions ────────────────────────────────────────────────────────────

    fn function(
        &mut self,
        id: Id<FunctionDef>,
        class_method: Option<ClassMethodContext<'_>>,
    ) -> Option<ast::FunctionDef> {
        let v = self.v;
        let func = *v.node(id);
        let span = span_from(v.token_range(func.kw.idx), v.range_of(id));
        let Ok(name_id) = func.name else {
            // The parser reported the missing name.
            return None;
        };
        let name = v.name(name_id);
        let name_span = v.name_range(name_id);

        let generic_params = self.generic_params(func.generics);
        let parameter_context = format!("function `{}`", name.as_str());
        let (mut params, mut defaults) = match func.params {
            Ok(params) => body::lower_params(
                v,
                params,
                name.as_str(),
                &parameter_context,
                &mut self.diags,
                true,
                &mut self.env_var_refs,
            ),
            Err(_) => (Vec::new(), FunctionDefaults::empty()),
        };
        let return_type = self.return_type(func.ret, &name);
        let throws = func
            .throws
            .and_then(|throws| v.node(throws).ty.ok())
            .map(|ty| {
                let te_span = v.range_any(&ty);
                let mut expr = types::lower_type(v, &mut self.diags, ty);
                check_throws_wildcard(&mut expr, te_span, &mut self.diags);
                expr.with_span(te_span)
            });

        let (body, declarative_meta) = match func.body {
            Ok(AnyFunctionBody::Llm(llm)) => self.llm_function(
                llm,
                &name,
                &generic_params,
                &mut params,
                &mut defaults,
                return_type.clone(),
                class_method,
            ),
            Ok(AnyFunctionBody::Expr(block)) => match builtin_kind(v, block) {
                Some(kind) => (Some(FunctionBodyDef::Builtin(kind)), None),
                None => {
                    let (expr_body, source_map, diags, env) = body::lower_body(v.tree, block);
                    self.diags.extend(diags);
                    self.env_var_refs.extend(env);
                    (Some(FunctionBodyDef::Expr(expr_body, source_map)), None)
                }
            },
            Ok(AnyFunctionBody::Bogus(_)) | Err(_) => (None, None),
        };

        let attributes = Vec::new();
        let docstring = self.docstring(func.docs);
        let header_title = self.header_title(func.docs);
        let is_tagged_template_tag = self.has_directive(func.docs, DirectiveKind::TaggedString);
        let trace_hook = self.trace_hook(func.docs);
        let explicit_trace_passthrough =
            self.has_directive(func.docs, DirectiveKind::TracePassthrough);

        Some(ast::FunctionDef {
            name,
            generic_params,
            params,
            defaults,
            return_type,
            throws,
            body,
            declarative_meta: declarative_meta.clone(),
            metadata: FunctionMetadata {
                trace_passthrough: declarative_meta.is_some() || explicit_trace_passthrough,
                explicit_trace_passthrough,
                ..FunctionMetadata::user_facing(FunctionOrigin::UserDefined)
            },
            attributes,
            docstring,
            header_title,
            is_tagged_template_tag,
            trace_hook,
            span,
            name_span,
        })
    }

    /// An LLM function's declarative body: the `@spec` companion body is
    /// synthesized while the prompt is in hand, the `client`/`on_event`
    /// overrides are appended to the parameters, and the function's own body
    /// becomes the agent run.
    #[expect(
        clippy::too_many_arguments,
        reason = "the pieces of one declaration, built in the rowan lowering's order"
    )]
    fn llm_function(
        &mut self,
        llm: Id<LlmBody>,
        name: &Name,
        generic_params: &[GenericParam],
        params: &mut Vec<Param>,
        defaults: &mut FunctionDefaults,
        return_type: Option<TypeExpr>,
        class_method: Option<ClassMethodContext<'_>>,
    ) -> (Option<FunctionBodyDef>, Option<DeclarativeMeta>) {
        let v = self.v;
        let llm_span = v.range_of(llm);
        let mut client_field = None;
        let mut prompt_field = None;
        let mut tools_field = None;
        for &field in v.tree.list(v.node(llm).fields) {
            match field {
                AnyLlmField::Client(id) => {
                    client_field.get_or_insert(id);
                }
                AnyLlmField::Prompt(id) => {
                    prompt_field.get_or_insert(id);
                }
                AnyLlmField::Tools(id) => {
                    tools_field.get_or_insert(id);
                }
                AnyLlmField::Bogus(_) => {}
            }
        }
        let client_value = client_field.and_then(|id| v.node(id).value.ok());
        let tools_value = tools_field.and_then(|id| v.node(id).value.ok());
        let prompt_value = prompt_field.and_then(|id| v.node(id).value.ok());

        let mut llm_body_def = LlmBodyDef {
            client: client_value.and_then(|value| self.client_name(value)),
            companion_bodies: Vec::new(),
            prompt_spans: None,
            prompt_segments: Vec::new(),
            has_tools: match tools_value {
                None => false,
                Some(AnyExpr::Array(array)) => !v.tree.list(v.node(array).elements).is_empty(),
                Some(_) => true,
            },
            span: llm_span,
        };
        reject_reserved_llm_params(params, name.as_str(), &mut self.diags);

        // A prompt is a string literal: backtick (interpolating) or quoted
        // (inert). Both become the same tagged template; the parser rejects
        // every other value shape.
        let prompt = match prompt_value {
            Some(AnyStringLike::Backtick(id)) => Some(Prompt::Backtick(id)),
            // Jinja prompts are removed: the single-path world renders prompts
            // as plain backtick templates through the spec.
            Some(AnyStringLike::String(id))
                if matches!(
                    v.tree.tokens().kind(v.node(id).token.idx),
                    TokenKind::LegacyHashString | TokenKind::UnclosedHashString
                ) =>
            {
                self.diags.push(LoweringDiagnostic::LlmJinjaPromptRemoved {
                    span: v.range_of(id),
                });
                None
            }
            Some(AnyStringLike::String(id)) => Some(Prompt::Quoted(id)),
            Some(AnyStringLike::Bogus(_)) | None => None,
        };
        if let Some(Prompt::Quoted(id)) = prompt {
            let token = v.node(id).token.idx;
            warn_quoted_prompt_interpolation(v.text(token), v.range_of(id), &mut self.diags);
        }
        llm_body_def.prompt_spans = prompt.map(|prompt| match prompt {
            Prompt::Backtick(id) => {
                let mut code = Vec::new();
                interpolation_spans(v, v.node(id).parts, &mut code);
                LlmPromptSpans {
                    literal: v.range_of(id),
                    code,
                }
            }
            // A quoted prompt is inert: the whole literal is prose.
            Prompt::Quoted(id) => LlmPromptSpans {
                literal: v.range_of(id),
                code: Vec::new(),
            },
        });

        let client = self.llm_client(name.as_str(), client_value, llm_span);

        // The function's real parameters — the injected `client` and
        // `on_event` overrides are added below and are never part of the
        // spec's bound arguments.
        let user_params: Vec<Param> = params
            .iter()
            .filter(|p| !crate::companions::is_injected_llm_param(&p.name))
            .cloned()
            .collect();
        let param_names: Vec<Name> = user_params.iter().map(|p| p.name.clone()).collect();
        let generic_param_names: Vec<Name> = generic_params
            .iter()
            .map(|param| param.name.clone())
            .collect();
        let owner_generic_param_names: Vec<Name> = class_method
            .map(|owner| {
                owner
                    .generic_params
                    .iter()
                    .map(|param| param.name.clone())
                    .collect()
            })
            .unwrap_or_default();

        if let (Some(prompt), Some(client)) = (prompt, client) {
            let source = TypedLlmSpecSource {
                v,
                prompt,
                tools: tools_value,
                client: client_value,
            };
            let (spec_body, spec_sm, prompt_segments, spec_diags, spec_env_refs) =
                synthesize_llm_spec_body(
                    name.as_str(),
                    &param_names,
                    &generic_param_names,
                    class_method.map(|owner| owner.name),
                    &owner_generic_param_names,
                    &client,
                    return_type,
                    &source,
                    llm_span,
                );
            self.diags.extend(spec_diags);
            self.env_var_refs.extend(spec_env_refs);
            llm_body_def
                .companion_bodies
                .push(("spec".to_string(), (spec_body, spec_sm)));
            llm_body_def.prompt_segments = prompt_segments;
        }

        // Every LLM function runs the ai Agent loop; `client: ai.Client? =
        // null` is the compiler-injected per-call override. When the spec
        // could not be synthesized (migration diagnostics fired), the body is
        // omitted so the missing `<Fn>@spec` reference never cascades.
        append_spec_client_param(params, defaults, llm_span);
        append_spec_on_event_param(params, defaults, llm_span);
        let body = llm_body_def
            .companion_bodies
            .iter()
            .any(|(t, _)| t == "spec")
            .then(|| {
                let (expr_body, source_map) = synthesize_spec_agent_run_body(
                    name.as_str(),
                    &user_params,
                    &generic_param_names,
                    class_method.map(|owner| owner.name),
                    &owner_generic_param_names,
                    llm_span,
                );
                FunctionBodyDef::Expr(expr_body, source_map)
            });
        (body, Some(DeclarativeMeta::Llm(llm_body_def)))
    }

    /// The `client:` value as the name the rowan lowering recorded: a bare
    /// identifier or a quoted string; nothing for an expression.
    fn client_name(&self, value: AnyExpr) -> Option<Name> {
        match value {
            AnyExpr::Path(id) => {
                let path = *self.v.node(id);
                self.v
                    .tree
                    .list(path.segments)
                    .is_empty()
                    .then(|| self.v.name(path.head))
            }
            AnyExpr::String(id) => Some(Name::new(strip_string_delimiters(
                self.v.text(self.v.node(id).token.idx),
            ))),
            _ => None,
        }
    }

    /// Classify the `client:` value, reporting the unusable shapes. `None`
    /// means an authoritative error was emitted (or the parser already
    /// reported a missing value).
    fn llm_client(
        &mut self,
        function_name: &str,
        value: Option<AnyExpr>,
        span: TextRange,
    ) -> Option<LlmClient> {
        match value? {
            AnyExpr::String(id) => {
                let text = strip_string_delimiters(self.v.text(self.v.node(id).token.idx));
                classify_client_string(function_name, &text, span, &mut self.diags)
            }
            AnyExpr::Binary(id)
                if reject_unquoted_client_shorthand(
                    function_name,
                    &self.v.tree.text()[self.v.range_of(id)],
                    span,
                    &mut self.diags,
                ) =>
            {
                None
            }
            _ => Some(LlmClient::Expr),
        }
    }

    fn method_sig(&mut self, id: Id<MethodSig>) -> Option<MethodSigDef> {
        let v = self.v;
        let sig = *v.node(id);
        // The signature ends before the `;` it owns.
        let span = self.member_span(
            v.token_range(sig.kw.idx),
            v.range_of(id),
            sig.semi.map(|semi| semi.idx),
        );
        let Ok(name_id) = sig.name else {
            // The parser reported the missing name.
            return None;
        };
        let name = v.name(name_id);
        let name_span = v.name_range(name_id);
        let generic_params = self.generic_params(sig.generics);
        let parameter_context = format!("method signature `{}`", name.as_str());
        // A signature has no defaults: what it reads is reported, not kept.
        let mut no_env = Vec::new();
        let params = match sig.params {
            Ok(params) => {
                body::lower_params(
                    v,
                    params,
                    name.as_str(),
                    &parameter_context,
                    &mut self.diags,
                    false,
                    &mut no_env,
                )
                .0
            }
            Err(_) => Vec::new(),
        };
        let return_type = self.return_type(sig.ret, &name);
        let throws = sig
            .throws
            .and_then(|throws| v.node(throws).ty.ok())
            .map(|ty| {
                // A bodyless method signature has nothing to infer an open
                // `throws … | _` from, and its declared throws is compared
                // structurally during conformance checking — so reject ANY `_`
                // here (unlike a function with a body, where a top-level `_` is
                // the open slot).
                let te_span = v.range_any(&ty);
                let mut expr = types::lower_type(v, &mut self.diags, ty);
                check_wildcard_type(
                    &mut expr,
                    "a method signature `throws` clause",
                    te_span,
                    &mut self.diags,
                );
                expr.with_span(te_span)
            });
        Some(MethodSigDef {
            name,
            generic_params,
            params,
            defaults: FunctionDefaults::empty(),
            return_type,
            throws,
            attributes: Vec::new(),
            docstring: self.docstring(sig.docs),
            span,
            name_span,
        })
    }

    // ── Classes, enums, interfaces ───────────────────────────────────────────

    /// A class and the `implements` blocks written in its body, which are its
    /// own items (an in-body block is shorthand for `implements<…> I for C<…>`).
    fn class(&mut self, id: Id<ClassDef>) -> Option<(ast::ClassDef, Vec<ImplementsForDef>)> {
        let v = self.v;
        let class = *v.node(id);
        let span = span_from(v.token_range(class.kw.idx), v.range_of(id));
        let Ok(name_id) = class.name else {
            // The parser reported the missing name.
            return None;
        };
        let generic_params = self.generic_params(class.generics);
        let class_name = v.name(name_id);
        let owner_generic_param_names: Vec<Name> = generic_params
            .iter()
            .map(|param| param.name.clone())
            .collect();
        let self_type = ast::class_self_type(&class_name, &generic_params, v.name_range(name_id));
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut implements = Vec::new();
        let mut attributes = Vec::new();
        for &member in v.tree.list(class.members) {
            match member {
                AnyClassMember::Field(id) => fields.push(self.field(
                    id,
                    |field_name| format!("field `{class_name}.{field_name}`"),
                    "a class field type",
                )),
                AnyClassMember::Method(id) => {
                    let Some(func) = self.function(
                        id,
                        Some(ClassMethodContext {
                            name: &class_name,
                            generic_params: &generic_params,
                        }),
                    ) else {
                        continue;
                    };
                    let companions =
                        expand_companions(&func, Some(&class_name), &owner_generic_param_names);
                    methods.push(func);
                    methods.extend(companions);
                }
                AnyClassMember::Implements(id) => {
                    implements.extend(self.implements_block(id, &generic_params, &self_type));
                }
                AnyClassMember::BlockAttr(id) => attributes.extend(self.block_attribute(id)),
                // Reported by the parser.
                AnyClassMember::Bogus(_) => {}
            }
        }
        let mut class_def = ast::ClassDef {
            name: class_name.clone(),
            generic_params,
            fields,
            methods,
            attributes,
            docstring: self.docstring(class.docs),
            span,
            name_span: v.name_range(name_id),
        };
        // BEP-042: wrap a magic `cleanup(self) -> void` method in its run-once guard.
        maybe_inject_cleanup_guard(&mut class_def);
        Some((class_def, implements))
    }

    /// A class or interface field. A field with no type is already reported
    /// by the parser, so it recovers with the error sentinel rather than an
    /// optional type: an absent type is not a kind of type.
    fn field(
        &mut self,
        id: Id<Field>,
        missing_context: impl Fn(&str) -> String,
        wildcard_context: &str,
    ) -> FieldDef {
        let v = self.v;
        let field = *v.node(id);
        let name = v.name(field.name);
        let span = self.member_span(
            v.name_range(field.name),
            v.range_of(id),
            field.sep.map(|sep| sep.idx),
        );
        let type_expr = match field.ty {
            Ok(ty) => {
                self.declared_type(ty, missing_context(name.as_str()), Some(wildcard_context))
            }
            Err(_) => TypeExprKind::Error.at(span),
        };
        FieldDef {
            attributes: self.member_attributes(field.attrs),
            docstring: self.docstring(field.docs),
            name,
            type_expr,
            span,
            name_span: v.name_range(field.name),
        }
    }

    fn enum_def(&mut self, id: Id<EnumDef>) -> Option<ast::EnumDef> {
        let v = self.v;
        let enum_def = *v.node(id);
        let span = span_from(v.token_range(enum_def.kw.idx), v.range_of(id));
        let Ok(name_id) = enum_def.name else {
            // The parser reported the missing name.
            return None;
        };
        let mut variants = Vec::new();
        let mut attributes = Vec::new();
        for &member in v.tree.list(enum_def.members) {
            match member {
                AnyEnumMember::Variant(id) => variants.push(self.variant(id)),
                AnyEnumMember::BlockAttr(id) => attributes.extend(self.block_attribute(id)),
                AnyEnumMember::Bogus(_) => {}
            }
        }
        Some(ast::EnumDef {
            name: v.name(name_id),
            variants,
            attributes,
            docstring: self.docstring(enum_def.docs),
            span,
            name_span: v.name_range(name_id),
        })
    }

    fn variant(&self, id: Id<EnumVariant>) -> VariantDef {
        let v = self.v;
        let variant = *v.node(id);
        VariantDef {
            name: v.name(variant.name),
            attributes: self.member_attributes(variant.attrs),
            docstring: self.docstring(variant.docs),
            span: self.member_span(
                v.name_range(variant.name),
                v.range_of(id),
                variant.sep.map(|sep| sep.idx),
            ),
            name_span: v.name_range(variant.name),
        }
    }

    fn interface(&mut self, id: Id<InterfaceDef>) -> Option<ast::InterfaceDef> {
        let v = self.v;
        let iface = *v.node(id);
        let span = span_from(v.token_range(iface.kw.idx), v.range_of(id));
        let Ok(name_id) = iface.name else {
            // The parser reported the missing name.
            return None;
        };
        let iface_name = v.name(name_id);
        let generic_params = self.generic_params(iface.generics);

        let parents: Vec<AnyType> = iface
            .requires
            .and_then(|clause| v.node(clause).types.ok())
            .map(|types| {
                v.tree
                    .list(types.as_list())
                    .iter()
                    .map(|item| item.item)
                    .collect()
            })
            .unwrap_or_default();
        let requires = parents
            .into_iter()
            .map(|ty| {
                self.declared_type(
                    ty,
                    format!("requires clause of interface `{iface_name}`"),
                    Some("an interface `requires` clause"),
                )
            })
            .collect();

        let mut fields = Vec::new();
        let mut required_methods = Vec::new();
        let mut associated_types = Vec::new();
        let mut default_methods = Vec::new();
        let mut attributes = Vec::new();
        for &member in v.tree.list(iface.members) {
            match member {
                AnyInterfaceMember::Field(id) => fields.push(self.field(
                    id,
                    |field_name| format!("interface field `{iface_name}.{field_name}`"),
                    "an interface field type",
                )),
                AnyInterfaceMember::MethodSig(id) => required_methods.extend(self.method_sig(id)),
                AnyInterfaceMember::AssocType(id) => {
                    associated_types.extend(self.associated_type_decl(id));
                }
                AnyInterfaceMember::Method(id) => default_methods.extend(self.function(id, None)),
                AnyInterfaceMember::BlockAttr(id) => attributes.extend(self.block_attribute(id)),
                AnyInterfaceMember::Bogus(_) => {}
            }
        }

        Some(ast::InterfaceDef {
            name: iface_name,
            generic_params,
            requires,
            fields,
            associated_types,
            required_methods,
            default_methods,
            attributes,
            docstring: self.docstring(iface.docs),
            span,
            name_span: v.name_range(name_id),
        })
    }

    fn associated_type_decl(&mut self, id: Id<AssocTypeDecl>) -> Option<AssociatedTypeDef> {
        let v = self.v;
        let decl = *v.node(id);
        let span = self.member_span(
            v.token_range(decl.kw.idx),
            v.range_of(id),
            decl.sep.map(|sep| sep.idx),
        );
        let Ok(name_id) = decl.name else {
            // The parser reported the missing name.
            return None;
        };
        let name = v.name(name_id);
        let bound = decl
            .bound
            .and_then(|bound| bound.ty.ok())
            .map(|ty| self.declared_type(ty, format!("bound of associated type `{name}`"), None));
        let default = decl
            .default
            .and_then(|default| default.ty.ok())
            .map(|ty| self.declared_type(ty, format!("default of associated type `{name}`"), None));
        Some(AssociatedTypeDef {
            name,
            bound,
            default,
            span,
            name_span: v.name_range(name_id),
        })
    }

    fn associated_type_binding(
        &mut self,
        id: Id<AssocTypeDef>,
    ) -> Option<AssociatedTypeBindingDef> {
        let v = self.v;
        let def = *v.node(id);
        let span = self.member_span(
            v.token_range(def.kw.idx),
            v.range_of(id),
            def.sep.map(|sep| sep.idx),
        );
        let Ok(name_id) = def.name else {
            // The parser reported the missing name.
            return None;
        };
        let name = v.name(name_id);
        let type_expr = def
            .ty
            .ok()
            .map(|ty| self.declared_type(ty, format!("binding of associated type `{name}`"), None));
        Some(AssociatedTypeBindingDef {
            name,
            type_expr,
            span,
            name_span: v.name_range(name_id),
        })
    }

    // ── Implementations ──────────────────────────────────────────────────────

    /// The last segment of an interface target's path, for diagnostics.
    fn interface_label(target: &TypeExpr) -> String {
        match &target.kind {
            TypeExprKind::Path { path, .. } => path
                .last()
                .map_or_else(|| "?".to_string(), ToString::to_string),
            _ => "?".to_string(),
        }
    }

    /// The members an `implements` block or `implements … for` shares: a
    /// field declared there is reported, the rest lower.
    fn impl_members(
        &mut self,
        members: List<AnyImplMember>,
        interface_label: &str,
    ) -> (
        Vec<ast::InterfaceFieldLinkDef>,
        Vec<AssociatedTypeBindingDef>,
        Vec<ast::FunctionDef>,
    ) {
        let v = self.v;
        let mut field_links = Vec::new();
        let mut associated_type_bindings = Vec::new();
        let mut methods = Vec::new();
        for &member in v.tree.list(members) {
            match member {
                AnyImplMember::Field(id) => {
                    let node = *v.node(id);
                    self.diags.push(
                        LoweringDiagnostic::InterfaceFieldDeclaredInImplementsBlock {
                            interface_name: interface_label.to_string(),
                            field_name: v.name(node.name).to_string(),
                            span: self.member_span(
                                v.name_range(node.name),
                                v.range_of(id),
                                node.sep.map(|sep| sep.idx),
                            ),
                        },
                    );
                }
                AnyImplMember::FieldLink(id) => {
                    field_links.extend(self.interface_field_link(id));
                }
                AnyImplMember::AssocType(id) => {
                    associated_type_bindings.extend(self.associated_type_binding(id));
                }
                AnyImplMember::Method(id) => methods.extend(self.function(id, None)),
                AnyImplMember::Bogus(_) => {}
            }
        }
        (field_links, associated_type_bindings, methods)
    }

    /// An in-body `implements I { … }` block as the `implements<…> I for C<…>`
    /// it is shorthand for: the class's generics, the class as the for-target.
    fn implements_block(
        &mut self,
        id: Id<ImplementsBlock>,
        class_generic_params: &[GenericParam],
        class_self_type: &TypeExpr,
    ) -> Option<ImplementsForDef> {
        let v = self.v;
        let block = *v.node(id);
        let target = self.declared_type(
            block.target.ok()?,
            "interface name in `implements`".to_string(),
            None,
        );
        let label = Self::interface_label(&target);
        let (field_links, associated_type_bindings, methods) =
            self.impl_members(block.members, &label);
        Some(ImplementsForDef {
            generic_params: class_generic_params.to_vec(),
            interface_target: target,
            for_target: class_self_type.clone(),
            field_links,
            associated_type_bindings,
            methods,
            origin: ImplOrigin::InBody,
            span: span_from(v.token_range(block.kw.idx), v.range_of(id)),
            docstring: None,
        })
    }

    fn implements_for(&mut self, id: Id<ImplementsFor>) -> Option<ImplementsForDef> {
        let v = self.v;
        let imp = *v.node(id);
        let generic_params = self.generic_params(imp.generics);
        let interface_target = self.declared_type(
            imp.target.ok()?,
            "interface name in `implements ... for`".to_string(),
            None,
        );
        let for_target = self.declared_type(
            imp.self_ty.ok()?,
            "target type in `implements ... for`".to_string(),
            None,
        );
        let label = Self::interface_label(&interface_target);
        let (field_links, associated_type_bindings, methods) =
            self.impl_members(imp.members, &label);
        Some(ImplementsForDef {
            generic_params,
            interface_target,
            for_target,
            field_links,
            associated_type_bindings,
            methods,
            origin: ImplOrigin::OutOfBody,
            span: span_from(v.token_range(imp.kw.idx), v.range_of(id)),
            docstring: self.docstring(imp.docs),
        })
    }

    fn interface_field_link(
        &mut self,
        id: Id<InterfaceFieldLink>,
    ) -> Option<ast::InterfaceFieldLinkDef> {
        let v = self.v;
        let link = *v.node(id);
        let span = self.member_span(
            v.name_range(link.iface_field),
            v.range_of(id),
            link.sep.map(|sep| sep.idx),
        );
        let Ok(class_field) = link.class_field else {
            // The parser reported the missing name.
            return None;
        };
        Some(ast::InterfaceFieldLinkDef {
            interface_field: v.name(link.iface_field),
            class_field: v.name(class_field),
            span,
            interface_field_span: v.name_range(link.iface_field),
            class_field_span: v.name_range(class_field),
        })
    }

    // ── Aliases, globals, clients ────────────────────────────────────────────

    fn type_alias(&mut self, id: Id<TypeAliasDef>) -> Option<ast::TypeAliasDef> {
        let v = self.v;
        let alias = *v.node(id);
        let span = span_from(v.token_range(alias.kw.idx), v.range_of(id));
        let Ok(name_id) = alias.name else {
            // The parser reported the missing name.
            return None;
        };
        let name = v.name(name_id);
        // `type A = ;` has a missing type; `type A` has none to lower.
        let type_expr = match (alias.eq, alias.ty) {
            (_, Ok(ty)) => {
                Some(self.declared_type(ty, format!("type alias `{name}`"), Some("a type alias")))
            }
            (Ok(_), Err(missing)) => Some(TypeExprKind::Missing.at(View::missing_range(missing))),
            (Err(_), Err(_)) => None,
        };
        Some(ast::TypeAliasDef {
            name,
            type_expr,
            span,
            name_span: v.name_range(name_id),
            docstring: self.docstring(alias.docs),
        })
    }

    /// `client Name = <expr>;` — a top-level client binding, the same
    /// [`LetOrigin::Client`] item the legacy config blocks produced, so the
    /// name resolves from every function body; the initializer is the user's
    /// own expression (evaluated once at `$init`).
    fn client_value(&mut self, id: Id<ClientValueDef>) -> LetDef {
        let v = self.v;
        let def = *v.node(id);
        let span = span_from(v.token_range(def.kw.idx), v.range_of(id));
        let mut ctx = LoweringContext::new();
        let root = body::expr_or_missing(&mut ctx, v, def.value, span);
        let (body, source_map, diags, env) = ctx.finish(Some(root));
        self.diags.extend(diags);
        self.env_var_refs.extend(env);
        LetDef {
            name: v.name(def.name),
            type_annotation: None,
            initializer: Some((body, source_map)),
            origin: LetOrigin::Client,
            docstring: self.docstring(def.docs),
            span,
            name_span: v.name_range(def.name),
        }
    }

    /// A file-scope `let`/`const`: exactly one name, `const NAME = value;`
    /// or, in source, `const NAME: Type = value;`. Everything a local `let`
    /// admits beyond that — a destructure, a wildcard, an alias chain, `let …
    /// else` — is refused, because a global is addressed by its one name from
    /// every body in the package and has no scope a refutable pattern could
    /// fall out of.
    fn top_level_let(&mut self, id: Id<LetItem>) -> Option<LetDef> {
        let v = self.v;
        let item = *v.node(id);
        let stmt = *v.node(item.stmt);
        let span = v.range_of(item.stmt);
        let surface = self.surface;

        // The binding's pattern and its initializer lower into separate
        // contexts: the initializer becomes the global's body, and a body that
        // also held the `NAME: Type` pattern would check that annotation a
        // second time as if a local declared it.
        let mut pattern_ctx = LoweringContext::new();
        if surface == TopLevelLetSurface::Source {
            let introducer = v.token_range(stmt.introducer.idx);
            // A `let` is refused, but the binding still lowers as the global
            // it was meant to be, so its readers resolve instead of cascading
            // errors.
            if v.tree.tokens().kind(stmt.introducer.idx) == TokenKind::Let {
                self.diags
                    .push(LoweringDiagnostic::TopLevelLetIntroducer { span: introducer });
            }
            pattern_ctx.file_scope_introducer = Some(introducer);
        }
        // A session binding is a local of the session, so `const` warns there
        // as it does in a body; only the source file's global introducer is
        // the one place `const` is right.
        body::warn_const_introducer(&mut pattern_ctx, v, stmt.introducer);
        let pattern = stmt
            .pattern
            .ok()
            .map(|pattern| body::pattern(&mut pattern_ctx, v, pattern));
        let patterns = std::mem::take(&mut pattern_ctx.patterns);
        let (_, _, pattern_diags, _) = pattern_ctx.finish(None);
        self.diags.extend(pattern_diags);

        let reject = |diags: &mut Vec<LoweringDiagnostic>, session: &'static str, source| {
            diags.push(match surface {
                TopLevelLetSurface::Session => LoweringDiagnostic::InvalidPatternAscription {
                    reason: session,
                    span,
                },
                TopLevelLetSurface::Source => LoweringDiagnostic::InvalidTopLevelLet {
                    reason: source,
                    span,
                },
            });
        };
        let Some(Pattern::Bind { name, subpat }) = pattern.map(|pattern| patterns[pattern].clone())
        else {
            reject(
                &mut self.diags,
                "session bindings require a single named binding",
                "a global variable binds a single name; destructuring and `_` are only \
                 available inside a body",
            );
            return None;
        };
        if stmt.else_branch.is_some() {
            reject(
                &mut self.diags,
                "session bindings do not support patterns or `else`",
                "`const … else` needs a body to leave; a global variable's initializer must \
                 produce its value",
            );
            return None;
        }
        let type_annotation = match (surface, subpat) {
            (_, None) => None,
            (TopLevelLetSurface::Session, Some(_)) => {
                self.diags
                    .push(LoweringDiagnostic::InvalidPatternAscription {
                        reason: "session bindings do not support patterns or `else`",
                        span,
                    });
                return None;
            }
            (TopLevelLetSurface::Source, Some(sub)) => {
                let Some(mut annotation) = ascription_type_expr(&patterns, sub) else {
                    self.diags.push(LoweringDiagnostic::InvalidTopLevelLet {
                        reason: "only a type annotation may follow a global variable's name",
                        span,
                    });
                    return None;
                };
                let annotation_span = annotation.span;
                check_wildcard_type(
                    &mut annotation,
                    "a global variable's type",
                    annotation_span,
                    &mut self.diags,
                );
                Some(annotation)
            }
        };
        let name_span = match stmt.pattern {
            Ok(AnyPattern::Binding(binding)) => v.name_range(v.node(binding).name),
            _ => span,
        };

        let mut ctx = LoweringContext::new();
        let initializer = stmt
            .init
            .ok()
            .map(|init| body::expression(&mut ctx, v, init));
        let (body, source_map, ctx_diags, ctx_env_refs) = ctx.finish(initializer);
        self.diags.extend(ctx_diags);
        self.env_var_refs.extend(ctx_env_refs);
        Some(LetDef {
            name,
            type_annotation,
            initializer: Some((body, source_map)),
            origin: LetOrigin::Source,
            docstring: match surface {
                TopLevelLetSurface::Source => self.docstring(item.docs),
                TopLevelLetSurface::Session => None,
            },
            span,
            name_span,
        })
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    /// A top-level `test`/`testset` as a registration; one with no name or
    /// no body (both reported by the parser) registers nothing.
    fn test_registration(&self, id: Id<TestDef>) -> Option<TypedTestRegistration<'t>> {
        let def = *self.v.node(id);
        Some(TypedTestRegistration {
            v: self.v,
            is_set: self.v.tree.tokens().kind(def.kw.idx) == TokenKind::Testset,
            name: def.name.ok()?,
            body: def.body.ok()?,
            with: def.with,
        })
    }

    // ── Removed forms ────────────────────────────────────────────────────────

    /// A removed top-level form the parser kept the shape of: the migration
    /// diagnostic the rowan lowering gave it, one per block, no cascade.
    fn removed_item(&mut self, id: Id<RemovedItem>) {
        let v = self.v;
        let item = *v.node(id);
        let span = v.range_of(id);
        let name = item.name.map(|name| v.name(name).to_string());
        match v.tree.tokens().kind(item.kw.idx) {
            // Legacy `client<llm> Name { ... }` config block: clients are
            // plain values now. The block's own `provider` names the client
            // class the suggested replacement constructs.
            TokenKind::Client => {
                let tokens = v.tree.tokens();
                let body: Vec<TokenIdx> = (item.body.start().0..item.body.end().0)
                    .map(TokenIdx)
                    .filter(|&idx| !tokens.kind(idx).is_trivia())
                    .collect();
                // The value after `provider` runs to the end of its line: a
                // quoted string, or the usual bare word (`vertex-ai`, which
                // lexes as several tokens).
                let provider = body
                    .windows(2)
                    .find(|pair| {
                        tokens.kind(pair[0]) == TokenKind::Word && v.text(pair[0]) == "provider"
                    })
                    .map(|pair| {
                        let start = usize::from(tokens.range(pair[1]).start());
                        let rest = &v.tree.text()[start..];
                        let line = rest.split('\n').next().unwrap_or(rest);
                        let value = line.trim().trim_end_matches(',').trim();
                        strip_string_delimiters(value)
                    });
                self.diags.push(LoweringDiagnostic::ClientBlockRemoved {
                    name: name.unwrap_or_else(|| "MyClient".to_string()),
                    provider,
                    span,
                });
            }
            // Legacy `retry_policy` block: retry composes at the client
            // boundary now (ai.Retry).
            TokenKind::RetryPolicy => self.diags.push(LoweringDiagnostic::RetryPolicyRemoved {
                name: name.unwrap_or_else(|| "policy".to_string()),
                span,
            }),
            // Generators are configured in `baml.toml`; the diagnostic points
            // at the `generator <name>` header, not the opaque body.
            TokenKind::Generator => {
                let kw = v.token_range(item.kw.idx);
                let span = item
                    .name
                    .map_or(kw, |name| span_from(kw, v.name_range(name)));
                self.diags
                    .push(LoweringDiagnostic::GeneratorBlockInBaml { name, span });
            }
            TokenKind::TemplateString => self
                .diags
                .push(LoweringDiagnostic::TemplateStringRemoved { span }),
            // The parser's E0098 is the whole report.
            _ => {}
        }
    }
}

/// A `$rust_function`-style body: a block whose only content is one of the
/// builtin markers.
fn builtin_kind(v: View<'_>, block: Id<Block>) -> Option<BuiltinKind> {
    let node = *v.node(block);
    if !v.tree.list(node.stmts).is_empty() {
        return None;
    }
    let AnyExpr::Path(path) = node.tail? else {
        return None;
    };
    let path = *v.node(path);
    if !v.tree.list(path.segments).is_empty() {
        return None;
    }
    Some(match v.text(v.node(path.head).token.idx) {
        "$rust_function" => BuiltinKind::Vm,
        "$rust_io_function" => BuiltinKind::Io,
        "$compiler_intrinsic" => BuiltinKind::Intrinsic,
        "$await_any" => BuiltinKind::AwaitAny,
        "$spawn" => BuiltinKind::Spawn,
        _ => return None,
    })
}

/// Every `${…}` interpolation's range in a template, nested ones included.
fn interpolation_spans(v: View<'_>, parts: List<AnyTemplatePart>, out: &mut Vec<TextRange>) {
    for &part in v.tree.list(parts) {
        match part {
            AnyTemplatePart::Interp(id) => out.push(v.range_of(id)),
            AnyTemplatePart::For(id) => interpolation_spans(v, v.node(id).body, out),
            AnyTemplatePart::If(id) => {
                let node = *v.node(id);
                interpolation_spans(v, node.then_parts, out);
                for &elif in v.tree.list(node.elifs) {
                    interpolation_spans(v, v.node(elif).parts, out);
                }
                if let Some(else_part) = node.else_part {
                    interpolation_spans(v, v.node(else_part).parts, out);
                }
            }
            AnyTemplatePart::Chunk(_) | AnyTemplatePart::Bogus(_) => {}
        }
    }
}

/// The prompt literal shapes an LLM function accepts.
#[derive(Clone, Copy)]
enum Prompt {
    Backtick(Id<BacktickString>),
    Quoted(Id<StringLit>),
}

/// The typed tree as a spec source.
struct TypedLlmSpecSource<'t> {
    v: View<'t>,
    prompt: Prompt,
    tools: Option<AnyExpr>,
    client: Option<AnyExpr>,
}

impl LlmSpecSource for TypedLlmSpecSource<'_> {
    fn prompt_span(&self) -> TextRange {
        match self.prompt {
            Prompt::Backtick(id) => self.v.range_of(id),
            Prompt::Quoted(id) => self.v.range_of(id),
        }
    }

    fn prompt_segments(&self, ctx: &mut LoweringContext) -> Vec<TemplateSegment> {
        match self.prompt {
            Prompt::Backtick(id) => templates::template_segments(ctx, self.v, id),
            // A quoted prompt is the degenerate template: regular string
            // literals do not interpolate, so the decoded value is one text
            // segment.
            Prompt::Quoted(id) => vec![TemplateSegment::Text(strip_string_delimiters(
                self.v.text(self.v.node(id).token.idx),
            ))],
        }
    }

    fn tools(
        &self,
        ctx: &mut LoweringContext,
        span: TextRange,
        wrap_bare_ref: &mut dyn FnMut(&mut LoweringContext, ExprId) -> ExprId,
    ) -> Option<ExprId> {
        Some(match self.tools? {
            AnyExpr::Array(array) => {
                let elements: Vec<_> = self.v.tree.list(self.v.node(array).elements).to_vec();
                let elements = elements
                    .into_iter()
                    .map(|element| {
                        // A bare function reference: normalized through ai.tool().
                        let is_bare_ref = matches!(element.value, AnyExpr::Path(_));
                        let id = body::expression(ctx, self.v, element.value);
                        if is_bare_ref {
                            wrap_bare_ref(ctx, id)
                        } else {
                            id
                        }
                    })
                    .collect();
                ctx.alloc_expr(Expr::Array { elements }, span)
            }
            // A non-list expression must already produce an `ai.Tool[]`.
            tools => body::expression(ctx, self.v, tools),
        })
    }

    fn client_expr(&self, ctx: &mut LoweringContext, span: TextRange) -> ExprId {
        match self.client {
            Some(client) => body::expression(ctx, self.v, client),
            None => ctx.alloc_expr(Expr::Missing, span),
        }
    }
}

/// A top-level `test`/`testset` of the typed tree.
struct TypedTestRegistration<'t> {
    v: View<'t>,
    is_set: bool,
    name: AnyExpr,
    body: Id<Block>,
    with: Option<Id<WithClause>>,
}

impl TestRegistration for TypedTestRegistration<'_> {
    fn is_set(&self) -> bool {
        self.is_set
    }

    fn body_span(&self) -> TextRange {
        self.v.range_of(self.body)
    }

    fn lower_name(&self, ctx: &mut InitTestContext) -> ExprId {
        body::expression(&mut ctx.inner, self.v, self.name)
    }

    fn lower_body(&self, ctx: &mut InitTestContext, _span: TextRange) -> ExprId {
        if self.is_set {
            body::testset_collector_body(&mut ctx.inner, self.v, self.body, Name::new("testset"))
        } else {
            body::lambda_body(&mut ctx.inner, self.v, self.body)
        }
    }

    fn lower_runner(&self, ctx: &mut InitTestContext, span: TextRange) -> ExprId {
        body::test_runner(&mut ctx.inner, self.v, self.with, span)
    }
}
