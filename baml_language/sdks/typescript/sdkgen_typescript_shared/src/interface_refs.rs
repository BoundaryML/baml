//! Checked callers use the compiler's resolved existential surface.
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use baml_codegen_types::{
    InterfaceCallability, InterfaceDeclaration, SymbolPool, project_interface_type,
};
use baml_type::{ParamTy, Ty as SemanticTy};

use crate::{
    translate_ty::{TranslateCtx, TranslatedType, translate_input_ty, translate_ty},
    ts_string,
};

#[derive(Default)]
pub(crate) struct RenderedRefs {
    pub source: String,
    pub signatures: Vec<TranslatedType>,
    pub uses_type_tokens: bool,
}

fn variable(index: usize, name: String) -> SemanticTy {
    SemanticTy::TypeVar(
        ParamTy::new(
            index.try_into().expect("type parameter index"),
            baml_base::Name::new(name),
        ),
        baml_base::TyAttr::EMPTY,
    )
}

fn generic_decl(names: &[String]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!("<{}>", names.join(", "))
    }
}

fn missing_required_pins(
    declaration: &InterfaceDeclaration,
    method: &baml_codegen_types::InterfaceMethod,
) -> bool {
    let mut missing = BTreeSet::new();
    for ty in method
        .params
        .iter()
        .map(|p| &p.ty)
        .chain([&method.return_type, &method.callable_throws])
    {
        let _ = baml_type::unify::rewrite_ty(&SemanticTy::from(ty.clone()), &mut |node| {
            if let SemanticTy::AssociatedTypeProjection {
                base, interface, ..
            } = node
                && matches!(base.as_ref(), SemanticTy::TypeVar(p, _) if p == &declaration.self_param)
                && interface.name != declaration.name
            {
                missing.insert(interface.name.clone());
            }
            None
        });
    }
    missing.len() == 1
        && declaration.callers.methods.iter().any(|caller| {
            caller.method.name == method.name && missing.contains(&caller.declaring_interface.name)
        })
}

pub(crate) fn render(pool: &SymbolPool, ctx: &TranslateCtx) -> RenderedRefs {
    let names = ctx.interfaces.as_ref().expect("interface names");
    let mut rendered = RenderedRefs::default();
    let Some(helpers) = names.helpers.get(&ctx.current_leaf) else {
        return rendered;
    };
    let witnesses = &helpers.witnesses;
    let interface_base = &helpers.interface_base;
    for declaration in pool
        .interfaces
        .declarations
        .values()
        .filter(|d| crate::routing::route_class_ref(&d.name) == ctx.current_leaf)
    {
        let class_name = &names.declarations[&declaration.name].reference;
        let mut parameters = BTreeMap::new();
        let mut associated = BTreeMap::new();
        let mut class_vars = Vec::new();
        let mut reserved = helpers.reserved.clone();
        for (index, parameter) in declaration.generic_params.iter().enumerate() {
            let name = crate::interface_names::allocate(format!("_P{index}"), &mut reserved);
            parameters.insert(parameter.param.clone(), variable(index, name.clone()));
            class_vars.push(name);
        }
        for (index, binding) in declaration.associated_types.iter().enumerate() {
            let name = crate::interface_names::allocate(format!("_A{index}"), &mut reserved);
            associated.insert(
                binding.name.clone(),
                variable(class_vars.len(), name.clone()),
            );
            class_vars.push(name);
        }
        let input_name = &names.declarations[&declaration.name].input;
        let own_proof = &names.declarations[&declaration.name].proof;
        writeln!(rendered.source, "\nexport interface {input_name}{} {{ readonly [{witnesses}.{own_proof}]: (view: {witnesses}.{own_proof}Types{}) => void; }}", generic_decl(&class_vars), generic_decl(&class_vars)).unwrap();
        let mut proofs = BTreeMap::<String, Vec<String>>::new();
        proofs.entry(own_proof.clone()).or_default().push(format!(
            "{witnesses}.{own_proof}Types{}",
            generic_decl(&class_vars)
        ));
        for required in &declaration.requires {
            let target = &names.declarations[&required.name];
            let Some(arguments) = view_arguments(required, names) else {
                continue;
            };
            let args = arguments
                .iter()
                .map(|ty| {
                    let ty = project_interface_type(ty, declaration, &parameters, &associated)
                        .unwrap_or_else(|error| {
                            panic!(
                                "cannot project required input {}: {error}",
                                declaration.name
                            )
                        });
                    let translated = translate_ty(&ty, ctx);
                    let expr = translated.expr.clone();
                    rendered.signatures.push(translated);
                    expr
                })
                .collect::<Vec<_>>();
            proofs
                .entry(target.proof.clone())
                .or_default()
                .push(format!(
                    "{witnesses}.{}Types{}",
                    target.proof,
                    generic_decl(&args)
                ));
        }
        writeln!(
            rendered.source,
            "\nexport class {class_name}{} extends {interface_base} {{",
            generic_decl(&class_vars)
        )
        .unwrap();
        write_proofs(&mut rendered.source, &proofs, true, witnesses);
        let type_class = &helpers.interface_type;
        let typed_type = &helpers.typed_type;
        let tokens = class_vars
            .iter()
            .enumerate()
            .map(|(index, ty)| format!("_type{index}: {typed_type}<{ty}>"))
            .collect::<Vec<_>>();
        let ordinary = (0..declaration.generic_params.len())
            .map(|index| format!("_type{index}"))
            .collect::<Vec<_>>();
        let bindings = declaration
            .associated_types
            .iter()
            .enumerate()
            .map(|(index, binding)| {
                format!(
                    "[{}, _type{}]",
                    ts_string(binding.name.as_str()),
                    declaration.generic_params.len() + index
                )
            })
            .collect::<Vec<_>>();
        writeln!(rendered.source, "  /** Select ordinary type arguments, then associated bindings, in declaration order. */").unwrap();
        writeln!(
            rendered.source,
            "  static type{}({}): {type_class}<{class_name}{}> {{",
            generic_decl(&class_vars),
            tokens.join(", "),
            generic_decl(&class_vars)
        )
        .unwrap();
        writeln!(rendered.source, "    if (arguments.length !== {}) throw new TypeError(\"interface type requires exactly {} arguments\");", tokens.len(), tokens.len()).unwrap();
        writeln!(
            rendered.source,
            "    return {type_class}._create<{class_name}{}>(this, {}, [{}], [{}]);",
            generic_decl(&class_vars),
            ts_string(&declaration.name.to_string()),
            ordinary.join(", "),
            bindings.join(", ")
        )
        .unwrap();
        writeln!(rendered.source, "  }}").unwrap();
        // The symbol-keyed input witnesses retain exact pins and interface
        // identity. An extra per-class private brand would incorrectly reject
        // a required interface's Ref surface, even when all its callers exist.
        for caller in &declaration.callers.methods {
            let method = &caller.method;
            if method.callability != Some(InterfaceCallability::Existential) {
                writeln!(
                    rendered.source,
                    "  // {} requires a concrete receiver or has no receiver.",
                    method.name
                )
                .unwrap();
                continue;
            }
            if missing_required_pins(declaration, method) {
                writeln!(
                    rendered.source,
                    "  // {} requires a checked {} view with explicit associated bindings.",
                    method.name, caller.declaring_interface.name
                )
                .unwrap();
                continue;
            }
            let mut parameters = parameters.clone();
            let mut method_vars = Vec::new();
            for (index, parameter) in method.generic_params.iter().enumerate() {
                let name = crate::interface_names::allocate(format!("_M{index}"), &mut reserved);
                parameters.insert(
                    parameter.param.clone(),
                    variable(class_vars.len() + index, name.clone()),
                );
                method_vars.push(name);
            }
            let mut project = |ty: &baml_type::RuntimeTy, input: bool| {
                let projected = project_interface_type(ty, declaration, &parameters, &associated)
                    .unwrap_or_else(|error| {
                        panic!(
                            "cannot generate TypeScript {}.{}: {error}",
                            declaration.name, method.name
                        )
                    });
                let native = if input {
                    translate_input_ty(&projected, ctx)
                } else {
                    translate_ty(&projected, ctx)
                };
                let expr = native.expr.clone();
                rendered.signatures.push(native);
                expr
            };
            let mut args = Vec::new();
            let mut fields = vec!["$ctx?: BamlCallContext | undefined".to_owned()];
            let mut wire = Vec::new();
            for (index, parameter) in method.params.iter().skip(1).enumerate() {
                let ty = project(&parameter.ty, true);
                let ty = if method_vars.is_empty() {
                    ty
                } else {
                    format!("{}<{ty}>", helpers.no_infer)
                };
                let wire_name = parameter
                    .name
                    .as_ref()
                    .expect("named interface argument")
                    .as_str();
                if parameter.mode == baml_type::FunctionParamMode::Optional {
                    fields.push(format!("{}?: {ty} | undefined", ts_string(wire_name)));
                    wire.push(format!(
                        "...($opts?.[{}] === undefined ? {{}} : {{ [{}]: $opts[{}] }})",
                        ts_string(wire_name),
                        ts_string(wire_name),
                        ts_string(wire_name)
                    ));
                } else {
                    args.push(format!("_arg{index}: {ty}"));
                    wire.push(format!("[{}]: _arg{index}", ts_string(wire_name)));
                }
            }
            let ret = project(&method.return_type, false);
            let error = project(&method.callable_throws, false).replace("*/", "* /");
            if !method_vars.is_empty() {
                rendered.uses_type_tokens = true;
                let choices = method
                    .generic_params
                    .iter()
                    .zip(&method_vars)
                    .map(|(p, native)| {
                        format!(
                            "{}: {}<{native}>",
                            ts_string(p.param.name().as_str()),
                            helpers.typed_type
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                fields.push(format!("$types: {{ {choices} }}"));
            }
            if !fields.is_empty() {
                args.push(format!(
                    "$opts{}: {{ {} }}",
                    if method_vars.is_empty() { "?" } else { "" },
                    fields.join("; ")
                ));
            }
            let tokens = if method_vars.is_empty() {
                ", undefined, [], $opts?.$ctx".to_owned()
            } else {
                let params = method
                    .generic_params
                    .iter()
                    .map(|p| ts_string(p.param.name().as_str()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(", $opts.$types, [{params}], $opts.$ctx")
            };
            let authored = declaration
                .callers
                .methods
                .iter()
                .map(|c| c.method.name.as_str())
                .collect();
            let native_name = method_name(method.name.as_str(), &authored);
            writeln!(
                rendered.source,
                "  /** Calls {}.{}; declared BAML throws: {error}. */",
                declaration.name, method.name
            )
            .unwrap();
            writeln!(
                rendered.source,
                "  [{}]{}({}): Promise<{ret}> {{",
                ts_string(&native_name),
                generic_decl(&method_vars),
                args.join(", ")
            )
            .unwrap();
            writeln!(
                rendered.source,
                "    return this._invoke({}, {{ {} }}{tokens}) as Promise<{ret}>;\n  }}",
                ts_string(method.name.as_str()),
                wire.join(", ")
            )
            .unwrap();
        }
        rendered.source.push_str("}\n");
    }
    for declaration in pool
        .interfaces
        .concrete_classes
        .values()
        .filter(|d| crate::routing::route_class_ref(&d.name) == ctx.current_leaf)
    {
        let Some(baml_codegen_types::Symbol::Class(class)) = pool.get(&declaration.name) else {
            continue;
        };
        if declaration.implemented_interfaces.is_empty() {
            continue;
        }
        let parameters: BTreeMap<_, _> = declaration
            .generic_params
            .iter()
            .zip(&class.generic_params)
            .map(|(p, name)| (p.param.clone(), name.clone()))
            .collect();
        let mut proofs = BTreeMap::<String, Vec<String>>::new();
        for view in &declaration.implemented_interfaces {
            let args = view_arguments(view, names)
                .expect("complete class implementation view")
                .iter()
                .map(|ty| {
                    let semantic =
                        baml_type::unify::rewrite_ty(&SemanticTy::from(ty.clone()), &mut |node| {
                            match node {
                                SemanticTy::TypeVar(p, attr) => parameters.get(p).map(|name| {
                                    SemanticTy::TypeVar(
                                        ParamTy::new(p.index(), name.clone()),
                                        attr.clone(),
                                    )
                                }),
                                _ => None,
                            }
                        });
                    let ty = baml_codegen_types::Ty::try_from(&semantic)
                        .expect("projectable implementation view");
                    let translated = translate_ty(&ty, ctx);
                    let expr = translated.expr.clone();
                    rendered.signatures.push(translated);
                    expr
                })
                .collect::<Vec<_>>();
            let proof = &names.declarations[&view.name].proof;
            proofs
                .entry(proof.clone())
                .or_default()
                .push(format!("{witnesses}.{proof}Types{}", generic_decl(&args)));
        }
        let generics = class
            .generic_params
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        writeln!(
            rendered.source,
            "\nexport interface {}{} {{",
            declaration.name.name(),
            generic_decl(&generics)
        )
        .unwrap();
        write_proofs(&mut rendered.source, &proofs, false, witnesses);
        rendered.source.push_str("}\n");
    }
    rendered
}

fn view_arguments(
    view: &baml_type::RuntimeInterface,
    names: &crate::interface_names::TypeScriptInterfaces,
) -> Option<Vec<baml_type::RuntimeTy>> {
    let mut args = view.generics.clone();
    for name in &names.declarations[&view.name].associated {
        args.push(
            view.associated_types
                .iter()
                .find(|(n, _)| n == name)?
                .1
                .clone(),
        );
    }
    Some(args)
}

fn write_proofs(
    out: &mut String,
    proofs: &BTreeMap<String, Vec<String>>,
    declare: bool,
    witnesses: &str,
) {
    for (proof, views) in proofs {
        let declarations = views
            .iter()
            .map(|view| format!("(view: {view}): void;"))
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(
            out,
            "  {}readonly [{witnesses}.{proof}]: {{ {declarations} }};",
            if declare { "declare " } else { "" }
        )
        .unwrap();
    }
}

pub(crate) fn render_witnesses(
    pool: &SymbolPool,
    names: &crate::interface_names::TypeScriptInterfaces,
) -> String {
    let mut out =
        String::from("// Internal type evidence. Native checked handles remain authoritative.\n");
    for (name, declaration) in &pool.interfaces.declarations {
        let proof = &names.declarations[name].proof;
        let params = (0..declaration.generic_params.len() + declaration.associated_types.len())
            .map(|i| format!("P{i}"))
            .collect::<Vec<_>>();
        writeln!(
            out,
            "export const {proof}: unique symbol = Symbol({});",
            ts_string(&name.to_string())
        )
        .unwrap();
        writeln!(
            out,
            "export interface {proof}Types{} {{ readonly pins: (value: [{}]) => [{}]; }}",
            generic_decl(&params),
            params.join(", "),
            params.join(", ")
        )
        .unwrap();
    }
    out
}

/// Avoid lifecycle/codec overrides and accidental Promise thenable assimilation.
pub(crate) fn method_name(name: &str, authored: &BTreeSet<&str>) -> String {
    let mut native = name.to_owned();
    if [
        "constructor",
        "then",
        "toJSON",
        "close",
        "clone",
        "as_interface",
        "_invoke",
        "_invokeConcrete",
        "_toHandle",
        "_concreteView",
    ]
    .contains(&name)
    {
        native.push('_');
        while authored.contains(native.as_str()) {
            native.push('_');
        }
    }
    native
}
