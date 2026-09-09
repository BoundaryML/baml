//! Interface caller classes. The compiler decides existential callability;
//! this module substitutes fixed associated bindings before native rendering.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    rc::Rc,
};

use baml_codegen_types::{
    InterfaceCallability, InterfaceDeclaration, Name, SymbolPool, Ty,
    project_interface_type as projected_type,
};
use baml_type::{ParamTy, RuntimeTy, Ty as SemanticTy};
use prost::Message;

use crate::{
    names::PythonNames,
    py_string,
    routing::LeafPath,
    translate_ty::{TranslateCtx, translate_ty},
};

fn variable(index: usize, name: String) -> SemanticTy {
    SemanticTy::TypeVar(
        ParamTy::new(
            index.try_into().expect("type parameter index"),
            baml_base::Name::new(name),
        ),
        baml_base::TyAttr::EMPTY,
    )
}

fn private_name(mut candidate: String, used: &mut BTreeSet<String>) -> String {
    while !used.insert(candidate.clone()) {
        candidate.push('_');
    }
    candidate
}

/// First ordinary binder surface. Do not erase unsatisfied type choices or
/// advanced operations to Any: those require their full descriptor/handler API.
fn ordinary_host_graph<'a>(
    pool: &'a SymbolPool,
    root: &'a InterfaceDeclaration,
) -> Option<Vec<&'a InterfaceDeclaration>> {
    let mut graph = vec![root];
    let mut seen = BTreeSet::from([root.name.clone()]);
    let mut methods = BTreeSet::new();
    let mut index = 0;
    while index < graph.len() {
        let declaration = graph[index];
        if !declaration.generic_params.is_empty()
            || !declaration.associated_types.is_empty()
            || !declaration.fields.is_empty()
            || !declaration.callers.ambiguous_methods.is_empty()
        {
            return None;
        }
        for method in declaration
            .required_methods
            .iter()
            .chain(&declaration.default_methods)
        {
            if !method.generic_params.is_empty()
                || method.callability != Some(InterfaceCallability::Existential)
                || !methods.insert(method.name.clone())
            {
                return None;
            }
            if method
                .params
                .iter()
                .skip(1)
                .map(|p| &p.ty)
                .chain(std::iter::once(&method.return_type))
                .any(|ty| {
                    projected_type(ty, declaration, &BTreeMap::new(), &BTreeMap::new()).is_err()
                })
            {
                return None;
            }
        }
        for required in &declaration.requires {
            if !required.generics.is_empty() || !required.associated_types.is_empty() {
                return None;
            }
            if seen.insert(required.name.clone()) {
                graph.push(pool.interfaces.declarations.get(&required.name)?);
            }
        }
        index += 1;
    }
    Some(graph)
}

fn host_surface(
    pool: &SymbolPool,
    declaration: &InterfaceDeclaration,
    names: &Rc<PythonNames>,
    leaf: &LeafPath,
    stub: bool,
    signature_types: &mut Vec<Ty>,
) -> Option<(String, String)> {
    let graph = ordinary_host_graph(pool, declaration)?;
    let host = names.interface_host(&declaration.name);
    let reference = names.interface_ref(&declaration.name);
    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: None,
        defer_name_refs: false,
        callback_protocols: None,
        type_stream_accessors: true,
        include_stream_done: stub,
        names: Some(names.clone()),
        type_var_names: BTreeMap::new(),
    };
    let mut out = format!("\nclass {host}(typing.Protocol):\n");
    let mut required_count = 0;
    let mut interfaces = Vec::new();
    for obligation in graph {
        let mut methods = Vec::new();
        for (method, required) in obligation
            .required_methods
            .iter()
            .map(|m| (m, true))
            .chain(obligation.default_methods.iter().map(|m| (m, false)))
        {
            let fqn = format!("{}.{}", declaration.name, method.name);
            let native = names.callable(&fqn, crate::names::BindingRole::DirectSync);
            let mut signature = vec!["self".to_owned()];
            let mut optional = Vec::new();
            let mut keyword = false;
            let mut positional = 0;
            for (index, parameter) in method.params.iter().skip(1).enumerate() {
                let wire = parameter
                    .name
                    .as_ref()
                    .map_or_else(|| format!("arg{index}"), ToString::to_string);
                let name = names.param(&fqn, &wire);
                let ty = projected_type(
                    &parameter.ty,
                    obligation,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                )
                .ok()?;
                let annotation = translate_ty(&ty, &ctx);
                signature_types.push(ty);
                if parameter.mode == baml_type::FunctionParamMode::Optional {
                    if !keyword {
                        signature.push("*".into());
                        keyword = true;
                    }
                    signature.push(format!("{name}: {annotation} = ..."));
                    optional.push(format!("({}, {})", py_string(&wire), py_string(&name)));
                } else {
                    positional += 1;
                    signature.push(format!("{name}: {annotation}"));
                }
            }
            if required {
                let ty = projected_type(
                    &method.return_type,
                    obligation,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                )
                .ok()?;
                let returns = crate::translate_ty::translate_input_ty(&ty, &ctx);
                signature_types.push(ty);
                writeln!(out, "    def {native}({}) -> typing.Union[{returns}, typing.Awaitable[{returns}]]: ...", signature.join(", ")).unwrap();
                required_count += 1;
            }
            let optional = if optional.is_empty() {
                "()".into()
            } else {
                format!("({},)", optional.join(", "))
            };
            methods.push(format!(
                "_HostMethod({}, {}, {}, {positional}, {optional})",
                py_string(method.name.as_str()),
                py_string(&native),
                if required { "True" } else { "False" }
            ));
        }
        let ty = baml_type::RuntimeTy::Interface(
            baml_type::TypeName::from_dotted_path(&obligation.name.to_string()),
            vec![],
            vec![],
            Default::default(),
        );
        let bytes = bridge_ctypes::runtime_ty_to_proto_ty(&ty).encode_to_vec();
        let methods = if methods.is_empty() {
            "()".into()
        } else {
            format!("({},)", methods.join(", "))
        };
        interfaces.push(format!("_HostInterface(bytes({bytes:?}), {methods})"));
    }
    if required_count == 0 {
        out.push_str("    pass\n");
    }
    let binder = if stub {
        format!(
            "    @staticmethod\n    async def bind(implementation: {host}) -> {reference}: ...\n"
        )
    } else {
        format!(
            "    @staticmethod\n    async def bind(implementation: {host}) -> {reference}:\n        from baml_sdk import _RUNTIME\n        from baml_sdk._typemap import _TYPE_MAP\n        return typing.cast({reference}, await _bind_interface_host(implementation, ({},), runtime=_RUNTIME, type_map=_TYPE_MAP))\n",
            interfaces.join(", ")
        )
    };
    Some((out, binder))
}

fn required_input_proofs(
    pool: &SymbolPool,
    names: &PythonNames,
    declaration: &InterfaceDeclaration,
    parameters: &BTreeMap<ParamTy, SemanticTy>,
    associated: &BTreeMap<baml_base::Name, SemanticTy>,
) -> Result<Vec<(String, Ty)>, String> {
    let mut proofs = Vec::new();
    // The compiler exports the transitive requires closure. A bound with an
    // unspecified associated type does not promise any particular existential
    // view; do not turn its declaration default into a pin here.
    for required in &declaration.requires {
        let target = pool
            .interfaces
            .declarations
            .get(&required.name)
            .ok_or_else(|| format!("missing required interface {}", required.name))?;
        let mut arguments = required.generics.clone();
        let pins = target
            .associated_types
            .iter()
            .map(|binding| {
                required
                    .associated_types
                    .iter()
                    .find(|(name, _)| name == &binding.name)
                    .map(|(_, ty)| ty.clone())
            })
            .collect::<Option<Vec<_>>>();
        let Some(pins) = pins else { continue };
        arguments.extend(pins);
        let witness = RuntimeTy::Class(
            names.interface_witness_name(&required.name),
            arguments,
            baml_base::TyAttr::EMPTY,
        );
        proofs.push((
            PythonNames::interface_proof(&required.name),
            projected_type(&witness, declaration, parameters, associated)?,
        ));
    }
    Ok(proofs)
}

/// A required view can leave the declaring interface's own associated pins
/// unspecified. Its ordinary native caller then needs explicit qualification
/// to that fully specified Ref. This is not an Any projection or a generic
/// fallback for arbitrary dependent types.
fn required_specialization(
    declaration: &InterfaceDeclaration,
    method: &baml_codegen_types::InterfaceMethod,
    names: &PythonNames,
) -> Option<String> {
    if method.callability != Some(InterfaceCallability::Existential) {
        return None;
    }
    let caller = declaration
        .callers
        .methods
        .iter()
        .find(|caller| caller.method.name == method.name)?;
    let mut unresolved = BTreeSet::new();
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
                unresolved.insert(interface.name.clone());
            }
            None
        });
    }
    if unresolved.len() != 1 || !unresolved.contains(&caller.declaring_interface.name) {
        return None;
    }
    Some(format!(
        "{} requires associated bindings from {}. Use await value.as_interface({}[...]) with all interface arguments and associated types, then call {} on the returned ref.",
        method.name,
        caller.declaring_interface.name,
        names.interface_ref(&caller.declaring_interface.name),
        method.name,
    ))
}

pub(crate) fn projection_guide(pool: &SymbolPool, names: &PythonNames) -> Option<String> {
    let mut rows = Vec::new();
    for declaration in pool.interfaces.declarations.values() {
        for caller in &declaration.callers.methods {
            if let Some(reason) = required_specialization(declaration, &caller.method, names) {
                rows.push(format!(
                    "- `{}.{}`: {}",
                    names.interface_ref(&declaration.name),
                    caller.method.name,
                    reason
                ));
            }
        }
    }
    (!rows.is_empty()).then(|| format!(
        "# Interface methods requiring associated-type selection\n\nThese methods have no direct caller on an unrefined Python ref. Select a fully specified generated interface Ref with `await value.as_interface(TargetRef[...])`. The SDK checks the chosen types against the existing receiver; it does not copy it or register another implementation. Incorrect choices fail before any receiver method runs.\n\n{}\n",
        rows.join("\n"),
    ))
}

pub(crate) fn render(
    pool: &SymbolPool,
    names: &Rc<PythonNames>,
    leaf: &LeafPath,
    stub: bool,
) -> String {
    let declarations = pool
        .interfaces
        .declarations
        .values()
        .filter(|d| names.route_class_ref(&d.name) == *leaf)
        .collect::<Vec<_>>();
    if declarations.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\nimport typing\nfrom baml_bridge._interface import BamlInterfaceRef as _BamlInterfaceRef\nfrom baml_bridge._host import HostMethod as _HostMethod, HostInterface as _HostInterface, bind_host as _bind_interface_host\nfrom baml_bridge import UNSET as _InterfaceUnset\n",
    );
    let mut callback_definitions = String::new();
    let mut signature_types = Vec::new();
    let mut used: BTreeSet<String> = pool
        .iter()
        .filter(|(name, symbol)| names.route(name, symbol) == *leaf)
        .flat_map(|(name, symbol)| match symbol {
            baml_codegen_types::Symbol::Function(_) => vec![
                names
                    .callable(&name.to_string(), crate::names::BindingRole::DirectSync)
                    .into_owned(),
                names
                    .callable(&name.to_string(), crate::names::BindingRole::DirectAsync)
                    .into_owned(),
            ],
            _ => vec![names.symbol(name).into_owned()],
        })
        .collect();
    for declaration in &declarations {
        used.insert(names.symbol(&declaration.name).into_owned());
        used.insert(names.interface_ref(&declaration.name));
        used.insert(names.interface_input(&declaration.name));
        used.insert(names.interface_witness(&declaration.name));
        used.insert(names.interface_host(&declaration.name));
    }
    for declaration in declarations {
        let class_name = names.interface_ref(&declaration.name);
        let mut params = BTreeMap::new();
        let mut associated = BTreeMap::new();
        let mut generic_names = Vec::new();
        for (index, param) in declaration.generic_params.iter().enumerate() {
            let native = private_name(format!("_{class_name}_P{index}"), &mut used);
            names.reserve_helper(leaf, &native);
            writeln!(out, "{native} = typing.TypeVar({})", py_string(&native)).unwrap();
            params.insert(param.param.clone(), variable(index, native.clone()));
            generic_names.push(native);
        }
        for (index, binding) in declaration.associated_types.iter().enumerate() {
            let native = private_name(format!("_{class_name}_A{index}"), &mut used);
            names.reserve_helper(leaf, &native);
            writeln!(out, "{native} = typing.TypeVar({})", py_string(&native)).unwrap();
            associated.insert(
                binding.name.clone(),
                variable(generic_names.len(), native.clone()),
            );
            generic_names.push(native);
        }
        // Method type variables are module declarations on Python 3.10+.
        let methods = declaration
            .callers
            .methods
            .iter()
            .map(|caller| &caller.method)
            .collect::<Vec<_>>();
        let mut method_variables = BTreeMap::new();
        for (method_index, method) in methods.iter().enumerate() {
            for (index, _) in method.generic_params.iter().enumerate() {
                let native =
                    private_name(format!("_{class_name}_M{method_index}_P{index}"), &mut used);
                names.reserve_helper(leaf, &native);
                writeln!(out, "{native} = typing.TypeVar({})", py_string(&native)).unwrap();
                method_variables.insert((method_index, index), native);
            }
        }
        let input_name = names.interface_input(&declaration.name);
        let witness = names.interface_witness(&declaration.name);
        let proof = PythonNames::interface_proof(&declaration.name);
        let args = if generic_names.is_empty() {
            String::new()
        } else {
            format!("[{}]", generic_names.join(", "))
        };
        let witness_base = if args.is_empty() {
            String::new()
        } else {
            format!("(typing.Generic{args})")
        };
        let proof_context = TranslateCtx {
            current_leaf: leaf.clone(),
            self_ref: None,
            defer_name_refs: false,
            callback_protocols: None,
            type_stream_accessors: true,
            include_stream_done: stub,
            names: Some(names.clone()),
            type_var_names: BTreeMap::new(),
        };
        let required_proofs = required_input_proofs(pool, names, declaration, &params, &associated)
            .unwrap_or_else(|error| {
                panic!("cannot generate Python input {}: {error}", declaration.name)
            });
        signature_types.extend(required_proofs.iter().map(|(_, ty)| ty.clone()));
        writeln!(out, "\nclass {witness}{witness_base}:\n    __slots__ = ()").unwrap();
        writeln!(out, "\nclass {input_name}(typing.Protocol{args}):\n    def {proof}(self, _view: {witness}{args}, /) -> None: ...").unwrap();
        writeln!(
            out,
            "{}",
            crate::leaf::render_input_proofs(&required_proofs, &proof_context, true)
        )
        .unwrap();
        let bases = if generic_names.is_empty() {
            "_BamlInterfaceRef".into()
        } else {
            format!(
                "_BamlInterfaceRef, typing.Generic[{}]",
                generic_names.join(", ")
            )
        };
        let host = host_surface(pool, declaration, names, leaf, stub, &mut signature_types);
        if let Some((source, _)) = &host {
            out.push_str(source);
        }
        writeln!(out, "\nclass {class_name}({bases}):",).unwrap();
        let guidance = methods
            .iter()
            .filter_map(|method| required_specialization(declaration, method, names))
            .collect::<Vec<_>>();
        if !guidance.is_empty() {
            writeln!(out, "    {}", py_string(&guidance.join("\n"))).unwrap();
        }
        writeln!(
            out,
            "    __slots__ = ()\n    __baml_interface_fqn__ = {}",
            py_string(&declaration.name.to_string())
        )
        .unwrap();
        writeln!(
            out,
            "    __baml_interface_generic_count__ = {}",
            declaration.generic_params.len()
        )
        .unwrap();
        writeln!(
            out,
            "    __baml_interface_associated_types__ = [{}]",
            declaration
                .associated_types
                .iter()
                .map(|a| py_string(a.name.as_str()))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        writeln!(
            out,
            "    def {proof}(self, _view: {witness}{args}, /) -> None: {}",
            if stub { "..." } else { "pass" }
        )
        .unwrap();
        writeln!(
            out,
            "{}",
            crate::leaf::render_input_proofs(&required_proofs, &proof_context, stub)
        )
        .unwrap();
        if let Some((_, binder)) = host {
            out.push_str(&binder);
        }
        for (method_index, method) in methods.iter().enumerate() {
            if let Some(reason) = required_specialization(declaration, method, names) {
                writeln!(out, "    # {reason}").unwrap();
                continue;
            }
            if method.callability != Some(InterfaceCallability::Existential) {
                // These operations are not callable on an existential in BAML.
                // Concrete-view codegen is a separate role, not an overload
                // accepting arbitrary refs on this class.
                writeln!(
                    out,
                    "    # {} requires a concrete type witness ({:?}).",
                    method.name, method.callability
                )
                .unwrap();
                continue;
            }
            let mut method_params = params.clone();
            for (index, param) in method.generic_params.iter().enumerate() {
                method_params.insert(
                    param.param.clone(),
                    variable(index, method_variables[&(method_index, index)].clone()),
                );
            }
            let mut callbacks = Vec::new();
            let mut seen_callbacks = std::collections::HashSet::new();
            if stub {
                for ty in method
                    .params
                    .iter()
                    .skip(1)
                    .map(|param| &param.ty)
                    .chain(std::iter::once(&method.return_type))
                {
                    let projected = projected_type(ty, declaration, &method_params, &associated)
                        .unwrap_or_else(|error| {
                            panic!(
                                "cannot generate callback type for {}.{}: {error}",
                                declaration.name, method.name
                            )
                        });
                    crate::leaf::collect_optional_callables(
                        &projected,
                        "callback",
                        &mut seen_callbacks,
                        &mut callbacks,
                    );
                }
            }
            let mut callback_map = indexmap::IndexMap::new();
            for (index, (ty, _)) in callbacks.iter().enumerate() {
                let preferred = format!("_{class_name}_M{method_index}_Callback{index}");
                let name = names.callback_helper(
                    leaf,
                    format!(
                        "interface callback:{}:{method_index}:{ty:?}",
                        declaration.name
                    ),
                    &preferred,
                    crate::translate_ty::callback_type_vars(ty).len(),
                );
                callback_map.insert(ty.clone(), name);
            }
            let ctx = TranslateCtx {
                current_leaf: leaf.clone(),
                self_ref: None,
                defer_name_refs: false,
                callback_protocols: (!callback_map.is_empty()).then(|| Rc::new(callback_map)),
                type_stream_accessors: true,
                include_stream_done: stub,
                names: Some(names.clone()),
                type_var_names: BTreeMap::new(),
            };
            if let Some(map) = &ctx.callback_protocols {
                for (ty, name) in map.iter() {
                    if let Ty::Function { params, ret, .. } = ty {
                        callback_definitions.push_str(&crate::leaf::render_callback_protocol(
                            name, params, ret, &ctx, false,
                        ));
                        callback_definitions.push_str(&crate::leaf::render_callback_protocol(
                            &format!("{name}Input"),
                            params,
                            ret,
                            &ctx,
                            true,
                        ));
                    }
                }
            }
            let mut render = |ty: &RuntimeTy, input: bool| {
                let projected = projected_type(ty, declaration, &method_params, &associated)
                    .unwrap_or_else(|error| {
                        panic!(
                            "cannot generate Python caller {}.{}: {error}",
                            declaration.name, method.name
                        )
                    });
                let rendered = if input {
                    crate::translate_ty::translate_input_ty(&projected, &ctx)
                } else {
                    translate_ty(&projected, &ctx)
                };
                signature_types.push(projected);
                rendered
            };
            let fqn = format!("{}.{}", declaration.name, method.name);
            let method_name = names.callable(&fqn, crate::names::BindingRole::DirectSync);
            let mut signature = vec!["self".to_owned()];
            let mut values = Vec::new();
            let mut optionals = Vec::new();
            let mut keyword_only = false;
            for (index, arg) in method.params.iter().skip(1).enumerate() {
                let wire_name = arg
                    .name
                    .as_ref()
                    .map_or_else(|| format!("arg{index}"), ToString::to_string);
                let arg_name = names.param(&fqn, &wire_name);
                let annotation = render(&arg.ty, true);
                if arg.mode == baml_type::FunctionParamMode::Optional {
                    if !keyword_only {
                        signature.push("*".into());
                        keyword_only = true;
                    }
                    signature.push(format!(
                        "{arg_name}: typing.Union[{annotation}, _InterfaceUnset] = _InterfaceUnset"
                    ));
                    optionals.push((wire_name, arg_name.into_owned()));
                } else {
                    signature.push(format!("{arg_name}: {annotation}"));
                    values.push(format!("{}: {arg_name}", py_string(&wire_name)));
                }
            }
            if !method.generic_params.is_empty() {
                if !keyword_only {
                    signature.push("*".into());
                }
                signature.push("_types: dict[str, typing.Any]".into());
            }
            let returns = render(&method.return_type, false);
            let errors = render(&method.callable_throws, false);
            if stub {
                writeln!(
                    out,
                    "\n    async def {method_name}({}) -> {returns}: ...",
                    signature.join(", ")
                )
                .unwrap();
            } else {
                writeln!(
                    out,
                    "\n    def {method_name}({}) -> typing.Awaitable[{returns}]:\n        {}\n        arguments = {{{}}}",
                    signature.join(", "),
                    py_string(&format!("BAML throws {errors}. Bridge failures remain possible.")),
                    values.join(", ")
                )
                .unwrap();
                for (wire_name, native) in optionals {
                    writeln!(out, "        if {native} is not _InterfaceUnset:\n            arguments[{}] = {native}", py_string(&wire_name)).unwrap();
                }
                let types = method
                    .generic_params
                    .iter()
                    .map(|p| py_string(p.param.as_str()))
                    .collect::<Vec<_>>();
                let types = if types.is_empty() {
                    "()".to_owned()
                } else {
                    format!("self._method_types(_types, ({},))", types.join(", "))
                };
                writeln!(
                    out,
                    "        return self._invoke({}, arguments, {types})",
                    py_string(method.name.as_str())
                )
                .unwrap();
            }
        }
    }
    out.push_str(&crate::leaf::interface_signature_imports(
        &signature_types,
        leaf,
        names,
    ));
    if out.contains("_BamlStream[") {
        out.push_str("\nfrom baml_bridge import BamlStream as _BamlStream\n");
    }
    if out.contains("_BamlPyHandle") {
        out.push_str("\nfrom baml_bridge import BamlPyHandle as _BamlPyHandle\n");
    }
    if !callback_definitions.is_empty() {
        out = format!("\nimport typing\nimport typing_extensions\n{callback_definitions}\n{out}");
    }

    out
}

pub(crate) fn exports(pool: &SymbolPool, names: &PythonNames, leaf: &LeafPath) -> String {
    let exported: Vec<_> = pool
        .interfaces
        .declarations
        .keys()
        .filter(|name| names.route_class_ref(name) == *leaf)
        .flat_map(|name| {
            let mut exports = vec![names.interface_ref(name), names.interface_input(name)];
            if ordinary_host_graph(pool, &pool.interfaces.declarations[name]).is_some() {
                exports.push(names.interface_host(name));
            }
            exports
        })
        .collect();
    if exported.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    writeln!(
        out,
        "\ntry:\n    __all__.extend([{}])\nexcept NameError:\n    __all__ = [{}]",
        exported
            .iter()
            .map(|n| py_string(n))
            .collect::<Vec<_>>()
            .join(", "),
        exported
            .iter()
            .map(|n| py_string(n))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    out
}

pub(crate) fn entries(pool: &SymbolPool, names: &PythonNames) -> Vec<(String, String, String)> {
    pool.interfaces
        .declarations
        .keys()
        .map(|name: &Name| {
            let leaf = names.route_class_ref(name);
            let module = std::iter::once("baml_sdk")
                .chain(leaf.segments.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(".");
            (name.to_string(), module, names.interface_ref(name))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_base::{Name as BaseName, TyAttr};
    use baml_codegen_types::{AssociatedType, InterfaceParameter};

    fn declaration() -> InterfaceDeclaration {
        InterfaceDeclaration {
            callers: baml_codegen_types::InterfaceCallers::default(),
            name: Name::new(BaseName::new("user"), vec![], BaseName::new("Source")),
            self_param: ParamTy::new(0, BaseName::new("Self")),
            generic_params: vec![InterfaceParameter {
                param: ParamTy::new(1, BaseName::new("T")),
                bounds: vec![],
            }],
            requires: vec![],
            associated_types: ["Output", "Error"]
                .into_iter()
                .map(|name| AssociatedType {
                    name: BaseName::new(name),
                    bound: None,
                    default: None,
                })
                .collect(),
            fields: vec![],
            required_methods: vec![],
            default_methods: vec![],
        }
    }

    #[test]
    fn ordinary_host_contract_and_binder_preserve_member_names_and_defaults() {
        use baml_codegen_types::{
            InterfaceCaller, InterfaceMethod, InterfaceMethodLinkage, InterfaceMethodTarget,
        };
        let mut declaration = declaration();
        declaration.generic_params.clear();
        declaration.associated_types.clear();
        let method = InterfaceMethod {
            name: BaseName::new("bind"),
            params: vec![
                baml_type::RuntimeFunctionParamTy {
                    name: Some("self".into()),
                    ty: RuntimeTy::TypeVar(declaration.self_param.clone(), TyAttr::EMPTY),
                    mode: baml_type::FunctionParamMode::Required,
                },
                baml_type::RuntimeFunctionParamTy {
                    name: Some("value".into()),
                    ty: RuntimeTy::string(),
                    mode: baml_type::FunctionParamMode::Required,
                },
            ],
            return_type: RuntimeTy::string(),
            declared_throws: None,
            callable_throws: RuntimeTy::Never {
                attr: TyAttr::EMPTY,
            },
            generic_params: vec![],
            target: InterfaceMethodTarget::Interface {
                interface: declaration.name.clone(),
                method: "bind".into(),
            },
            linkage: InterfaceMethodLinkage::Linkable,
            builtin: None,
            callability: Some(InterfaceCallability::Existential),
        };
        declaration.required_methods.push(method.clone());
        declaration.callers.methods.push(InterfaceCaller {
            declaring_interface: baml_type::RuntimeInterface::new(
                declaration.name.clone(),
                vec![],
                vec![],
            ),
            method,
        });
        let mut pool = SymbolPool::new();
        std::sync::Arc::make_mut(&mut pool.interfaces)
            .declarations
            .insert(declaration.name.clone(), declaration.clone());
        let names = Rc::new(PythonNames::build(&pool));
        let native = names.callable("user.Source.bind", crate::names::BindingRole::DirectSync);
        assert_ne!(
            native, "bind",
            "the administrative binder owns its public spelling"
        );
        let leaf = names.route_class_ref(&declaration.name);
        let source = render(&pool, &names, &leaf, false);
        assert!(source.contains("class SourceHost(typing.Protocol):"));
        assert!(source.contains(&format!(
            "def {native}(self, value: str) -> typing.Union[str, typing.Awaitable[str]]"
        )));
        assert!(source.contains("async def bind(implementation: SourceHost) -> SourceRef:"));
        assert!(source.contains(&format!(
            "_HostMethod(\"bind\", {}, True, 1",
            py_string(&native)
        )));
        assert!(exports(&pool, &names, &leaf).contains("SourceHost"));
        let stub = render(&pool, &names, &leaf, true);
        assert!(stub.contains("async def bind(implementation: SourceHost) -> SourceRef: ..."));
    }

    #[test]
    fn unpinned_required_caller_has_an_explicit_checked_projection_guide() {
        use baml_codegen_types::{
            InterfaceCaller, InterfaceMethod, InterfaceMethodLinkage, InterfaceMethodTarget,
        };
        let base = declaration();
        let mut root = declaration();
        root.name = Name::new("user".into(), vec![], "Open".into());
        root.generic_params.clear();
        root.associated_types.clear();
        let required =
            baml_type::RuntimeInterface::new(base.name.clone(), vec![RuntimeTy::string()], vec![]);
        root.requires.push(required.clone());
        let projection = |member| RuntimeTy::AssociatedTypeProjection {
            base: Box::new(RuntimeTy::TypeVar(root.self_param.clone(), TyAttr::EMPTY)),
            interface: Box::new(required.clone()),
            member: BaseName::new(member),
            attr: TyAttr::EMPTY,
        };
        let method = InterfaceMethod {
            name: BaseName::new("apply"),
            params: vec![
                baml_type::RuntimeFunctionParamTy {
                    name: Some(BaseName::new("self")),
                    ty: RuntimeTy::TypeVar(root.self_param.clone(), TyAttr::EMPTY),
                    mode: baml_type::FunctionParamMode::Required,
                },
                baml_type::RuntimeFunctionParamTy {
                    name: Some(BaseName::new("value")),
                    ty: RuntimeTy::string(),
                    mode: baml_type::FunctionParamMode::Required,
                },
            ],
            return_type: projection("Output"),
            declared_throws: None,
            callable_throws: projection("Error"),
            generic_params: vec![],
            target: InterfaceMethodTarget::Interface {
                interface: base.name.clone(),
                method: BaseName::new("apply"),
            },
            linkage: InterfaceMethodLinkage::Linkable,
            builtin: None,
            callability: Some(InterfaceCallability::Existential),
        };
        root.callers.methods.push(InterfaceCaller {
            declaring_interface: required,
            method: method.clone(),
        });
        let mut pool = SymbolPool::new();
        let graph = std::sync::Arc::make_mut(&mut pool.interfaces);
        graph.declarations.insert(base.name.clone(), base);
        graph.declarations.insert(root.name.clone(), root.clone());
        let names = Rc::new(PythonNames::build(&pool));
        let guide = projection_guide(&pool, &names).unwrap();
        assert!(guide.contains("OpenRef.apply"));
        assert!(guide.contains("as_interface(SourceRef[...])"));
        for stub in [false, true] {
            let rendered = render(&pool, &names, &names.route_class_ref(&root.name), stub);
            assert!(rendered.contains("apply requires associated bindings from user.Source"));
            assert!(
                !rendered.contains("def apply("),
                "no invented normal caller signature"
            );
        }
        let mut resolved = method.clone();
        resolved.return_type = RuntimeTy::string();
        resolved.callable_throws = RuntimeTy::Never {
            attr: TyAttr::EMPTY,
        };
        assert!(required_specialization(&root, &resolved, &names).is_none());
        let mut generic = method;
        for ty in [&mut generic.return_type, &mut generic.callable_throws] {
            if let RuntimeTy::AssociatedTypeProjection { base, .. } = ty {
                *base = Box::new(RuntimeTy::TypeVar(
                    ParamTy::new(1, BaseName::new("T")),
                    TyAttr::EMPTY,
                ));
            }
        }
        assert!(
            required_specialization(&root, &generic, &names).is_none(),
            "a generic method projection needs its own solution"
        );
    }

    #[test]
    fn projection_uses_slots_instead_of_shadowed_parameter_names() {
        let declaration = declaration();
        let outer = declaration.generic_params[0].param.clone();
        let inner = ParamTy::new(2, BaseName::new("T"));
        let input = RuntimeTy::map(
            RuntimeTy::TypeVar(outer.clone(), TyAttr::EMPTY),
            RuntimeTy::TypeVar(inner.clone(), TyAttr::EMPTY),
        );
        let parameters =
            BTreeMap::from([(outer, SemanticTy::string()), (inner, SemanticTy::int())]);
        let actual = projected_type(&input, &declaration, &parameters, &BTreeMap::new()).unwrap();
        assert_eq!(
            actual,
            Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::EMPTY
                }),
                value: Box::new(Ty::Int {
                    attr: TyAttr::EMPTY
                }),
                attr: TyAttr::EMPTY,
            }
        );
    }

    #[test]
    fn self_projection_does_not_erase_unrelated_generic_projections() {
        let declaration = declaration();
        let mut projection = RuntimeTy::AssociatedTypeProjection {
            base: Box::new(RuntimeTy::TypeVar(
                declaration.self_param.clone(),
                TyAttr::EMPTY,
            )),
            interface: Box::new(baml_type::RuntimeInterface::new(
                declaration.name.clone(),
                vec![],
                vec![],
            )),
            member: BaseName::new("Output"),
            attr: TyAttr::EMPTY,
        };
        let associated = BTreeMap::from([(BaseName::new("Output"), SemanticTy::string())]);
        assert_eq!(
            projected_type(
                &RuntimeTy::list(projection.clone()),
                &declaration,
                &BTreeMap::new(),
                &associated
            )
            .unwrap(),
            Ty::List(
                Box::new(Ty::String {
                    attr: TyAttr::EMPTY
                }),
                TyAttr::EMPTY
            )
        );
        if let RuntimeTy::AssociatedTypeProjection { base, .. } = &mut projection {
            *base = Box::new(RuntimeTy::TypeVar(
                ParamTy::new(2, BaseName::new("T")),
                TyAttr::EMPTY,
            ));
        }
        let error =
            projected_type(&projection, &declaration, &BTreeMap::new(), &associated).unwrap_err();
        assert!(error.contains("user.Source"));
        assert!(error.contains("AssociatedTypeProjection"));
    }

    #[test]
    fn required_views_preserve_pin_order_and_do_not_apply_constraint_defaults() {
        let base = declaration();
        let mut child = declaration();
        child.name = Name::new(BaseName::new("user"), vec![], BaseName::new("Child"));
        child.requires = vec![baml_type::RuntimeInterface::new(
            base.name.clone(),
            vec![RuntimeTy::int()],
            vec![
                (
                    BaseName::new("Error"),
                    RuntimeTy::Never {
                        attr: TyAttr::EMPTY,
                    },
                ),
                (BaseName::new("Output"), RuntimeTy::string()),
            ],
        )];
        let mut pool = SymbolPool::new();
        let graph = std::sync::Arc::make_mut(&mut pool.interfaces);
        graph.declarations.insert(base.name.clone(), base);
        graph.declarations.insert(child.name.clone(), child.clone());
        let names = Rc::new(PythonNames::build(&pool));
        let rendered = render(&pool, &names, &names.route_class_ref(&child.name), true);
        assert_eq!(
            rendered
                .matches("_view: _SourceView[int, str, typing.NoReturn]")
                .count(),
            2,
            "both ChildInput and ChildRef must carry the exact required view: {rendered}"
        );
        child.requires[0]
            .associated_types
            .retain(|(name, _)| name.as_str() != "Error");
        let proofs =
            required_input_proofs(&pool, &names, &child, &BTreeMap::new(), &BTreeMap::new())
                .unwrap();
        assert!(
            proofs.is_empty(),
            "an unpinned required Error is not its declaration default"
        );
    }

    #[test]
    fn ref_binding_order_follows_declaration_not_sorted_wire_pins() {
        let declaration = declaration();
        let name = declaration.name.clone();
        let mut pool = SymbolPool::new();
        std::sync::Arc::make_mut(&mut pool.interfaces)
            .declarations
            .insert(name.clone(), declaration);
        let names = Rc::new(PythonNames::build(&pool));
        let context = TranslateCtx {
            current_leaf: names.route_class_ref(&name),
            names: Some(names),
            self_ref: None,
            defer_name_refs: false,
            callback_protocols: None,
            type_stream_accessors: true,
            include_stream_done: true,
            type_var_names: BTreeMap::new(),
        };
        let ty = Ty::Interface(
            name,
            vec![Ty::Int {
                attr: TyAttr::EMPTY,
            }],
            vec![
                (
                    BaseName::new("Error"),
                    Ty::Never {
                        attr: TyAttr::EMPTY,
                    },
                ),
                (
                    BaseName::new("Output"),
                    Ty::String {
                        attr: TyAttr::EMPTY,
                    },
                ),
            ],
            TyAttr::EMPTY,
        );
        assert_eq!(
            translate_ty(&ty, &context),
            "SourceRef[int, str, typing.NoReturn]"
        );
    }
}
