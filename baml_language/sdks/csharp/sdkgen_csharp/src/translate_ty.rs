use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt::Write as _,
};

use baml_base::{Literal, MediaKind};
use baml_codegen_types::{CodegenFunctionParamMode, Name, Ty, TypeAlias};
use sha2::{Digest, Sha256};

use crate::{
    csharp_string,
    names::{allocate_scope, namespace_segment, parameter_name},
    routing::route,
};

#[derive(Default)]
pub(crate) struct AliasMap<'a> {
    aliases: HashMap<Name, &'a TypeAlias>,
    projected_type_names: HashMap<Name, String>,
    projected_namespaces: HashMap<Name, String>,
}

impl<'a> AliasMap<'a> {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn get(&self, name: &Name) -> Option<&&'a TypeAlias> {
        self.aliases.get(name)
    }

    #[cfg(test)]
    pub(crate) fn insert(&mut self, name: Name, alias: &'a TypeAlias) {
        self.aliases.insert(name, alias);
    }

    pub(crate) fn set_projected_type_name(&mut self, name: Name, projected: String) {
        self.projected_type_names.insert(name, projected);
    }

    pub(crate) fn set_projected_namespace(&mut self, name: Name, projected: String) {
        self.projected_namespaces.insert(name, projected);
    }

    pub(crate) fn projected_type_name(&self, name: &Name) -> String {
        self.projected_type_names
            .get(name)
            .cloned()
            .unwrap_or_else(|| preferred_type_name(name))
    }

    fn projected_namespace(&self, name: &Name) -> String {
        self.projected_namespaces
            .get(name)
            .cloned()
            .unwrap_or_else(|| route(name).namespace)
    }
}

pub(crate) fn preferred_type_name(name: &Name) -> String {
    let mut projected = namespace_segment(name.bare_name());
    if name.is_stream() {
        projected.push_str("Stream");
    }
    projected
}

impl<'a> FromIterator<(Name, &'a TypeAlias)> for AliasMap<'a> {
    fn from_iter<T: IntoIterator<Item = (Name, &'a TypeAlias)>>(iter: T) -> Self {
        Self {
            aliases: iter.into_iter().collect(),
            projected_type_names: HashMap::new(),
            projected_namespaces: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranslatedType {
    pub(crate) source: String,
    pub(crate) primitive_codec: bool,
    pub(crate) async_callback_source: Option<String>,
    pub(crate) contains_host_callable: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CallbackDelegate {
    pub(crate) sync_name: String,
    async_name: String,
    parameters: Vec<CallbackParameter>,
    return_type: String,
    returns_void: bool,
}

#[derive(Clone, Debug)]
struct CallbackParameter {
    wire_name: String,
    ty: String,
    optional: bool,
}

impl TranslatedType {
    fn primitive(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            primitive_codec: true,
            async_callback_source: None,
            contains_host_callable: false,
        }
    }

    fn stub(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            primitive_codec: false,
            async_callback_source: None,
            contains_host_callable: false,
        }
    }
}

pub(crate) fn translate(ty: &Ty, aliases: &AliasMap<'_>) -> TranslatedType {
    translate_with_type_variables(ty, aliases, &BTreeMap::new())
}

pub(crate) fn translate_with_type_variables(
    ty: &Ty,
    aliases: &AliasMap<'_>,
    type_variables: &BTreeMap<String, String>,
) -> TranslatedType {
    translate_inner(ty, aliases, type_variables, &mut HashSet::new())
}

#[cfg(test)]
pub(crate) fn translate_argument(
    ty: &Ty,
    aliases: &AliasMap<'_>,
    owner_identity: &str,
    argument_name: &str,
) -> TranslatedType {
    translate_argument_with_type_variables(
        ty,
        aliases,
        owner_identity,
        argument_name,
        &BTreeMap::new(),
    )
}

pub(crate) fn translate_argument_with_type_variables(
    ty: &Ty,
    aliases: &AliasMap<'_>,
    owner_identity: &str,
    argument_name: &str,
    type_variables: &BTreeMap<String, String>,
) -> TranslatedType {
    let Some(callback) = callback_delegate_with_type_variables(
        ty,
        aliases,
        owner_identity,
        argument_name,
        type_variables,
    ) else {
        return translate_with_type_variables(ty, aliases, type_variables);
    };

    TranslatedType {
        source: callback.sync_name,
        primitive_codec: true,
        async_callback_source: Some(callback.async_name),
        contains_host_callable: true,
    }
}

#[cfg(test)]
pub(crate) fn callback_delegate(
    ty: &Ty,
    aliases: &AliasMap<'_>,
    owner_identity: &str,
    argument_name: &str,
) -> Option<CallbackDelegate> {
    callback_delegate_with_type_variables(
        ty,
        aliases,
        owner_identity,
        argument_name,
        &BTreeMap::new(),
    )
}

pub(crate) fn callback_delegate_with_type_variables(
    ty: &Ty,
    aliases: &AliasMap<'_>,
    owner_identity: &str,
    argument_name: &str,
    type_variables: &BTreeMap<String, String>,
) -> Option<CallbackDelegate> {
    let Ty::Function {
        params,
        ret,
        throws,
        attr: _,
    } = ty
    else {
        return None;
    };
    let _ = throws;
    if !params
        .iter()
        .any(|parameter| parameter.mode == CodegenFunctionParamMode::Optional)
        || params.len() > 16
    {
        return None;
    }

    let translated_parameters = params
        .iter()
        .map(|parameter| translate_with_type_variables(&parameter.ty, aliases, type_variables))
        .collect::<Vec<_>>();
    let translated_return = translate_with_type_variables(ret, aliases, type_variables);
    if translated_parameters
        .iter()
        .any(|parameter| !parameter.primitive_codec)
        || (!matches!(ret.as_ref(), Ty::Void { .. }) && !translated_return.primitive_codec)
    {
        return None;
    }

    let identity = format!("{owner_identity}:callback:{argument_name}");
    let digest = Sha256::digest(identity.as_bytes());
    let suffix = digest.iter().fold(String::new(), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    });
    let mut callback_type_variables = BTreeSet::new();
    for parameter in params {
        collect_type_variables(&parameter.ty, &mut callback_type_variables);
    }
    collect_type_variables(ret, &mut callback_type_variables);
    let generic_parameters = callback_type_variables
        .iter()
        .map(|name| {
            type_variables
                .get(name)
                .cloned()
                .unwrap_or_else(|| namespace_segment(name))
        })
        .collect::<Vec<_>>();
    let generic_suffix = if generic_parameters.is_empty() {
        String::new()
    } else {
        format!("<{}>", generic_parameters.join(", "))
    };
    let base_name = format!("BamlCallback_{suffix}");
    let sync_name = format!("{base_name}{generic_suffix}");
    Some(CallbackDelegate {
        async_name: format!("{base_name}Async{generic_suffix}"),
        sync_name,
        parameters: params
            .iter()
            .zip(translated_parameters)
            .enumerate()
            .map(|(index, (parameter, translated))| CallbackParameter {
                wire_name: parameter
                    .name
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("arg{index}")),
                ty: translated.source,
                optional: parameter.mode == CodegenFunctionParamMode::Optional,
            })
            .collect(),
        return_type: translated_return.source,
        returns_void: matches!(ret.as_ref(), Ty::Void { .. }),
    })
}

impl CallbackDelegate {
    pub(crate) fn render(&self, out: &mut String) {
        let parameters = self.render_parameters();
        let sync_return = if self.returns_void {
            "void"
        } else {
            &self.return_type
        };
        let async_return = if self.returns_void {
            "global::System.Threading.Tasks.ValueTask".to_string()
        } else {
            format!(
                "global::System.Threading.Tasks.ValueTask<{}>",
                self.return_type
            )
        };
        let _ = writeln!(
            out,
            "public delegate {sync_return} {}({parameters});",
            self.sync_name
        );
        let _ = writeln!(
            out,
            "public delegate {async_return} {}({parameters});\n",
            self.async_name
        );
    }

    fn render_parameters(&self) -> String {
        let mut occupied = BTreeSet::new();
        let parameter_names = allocate_scope(
            self.parameters
                .iter()
                .enumerate()
                .map(|(index, parameter)| {
                    (
                        parameter_name(&parameter.wire_name),
                        format!(
                            "{}:parameter:{index}:{}",
                            self.sync_name, parameter.wire_name
                        ),
                    )
                }),
            &mut occupied,
        );
        self.parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                let projected = &parameter_names[&format!(
                    "{}:parameter:{index}:{}",
                    self.sync_name, parameter.wire_name
                )];
                let ty = if parameter.optional {
                    format!("global::Baml.BamlOptional<{}>", parameter.ty)
                } else {
                    parameter.ty.clone()
                };
                format!(
                    "[global::Baml.BamlWireNameAttribute({})] {ty} {projected}",
                    csharp_string(&parameter.wire_name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn collect_type_variables(ty: &Ty, names: &mut BTreeSet<String>) {
    match ty {
        Ty::TypeVar(name, _) => {
            names.insert(name.to_string());
        }
        Ty::List(inner, _) => collect_type_variables(inner, names),
        Ty::Map { key, value, .. } => {
            collect_type_variables(key, names);
            collect_type_variables(value, names);
        }
        Ty::Union(options, _) => {
            for option in options {
                collect_type_variables(option, names);
            }
        }
        Ty::Class(_, arguments, _) => {
            for argument in arguments {
                collect_type_variables(argument, names);
            }
        }
        Ty::Interface(_, generics, associated_types, _) => {
            for generic in generics {
                collect_type_variables(generic, names);
            }
            for (_, associated_type) in associated_types {
                collect_type_variables(associated_type, names);
            }
        }
        Ty::Function {
            params,
            ret,
            throws,
            attr: _,
        } => {
            for parameter in params {
                collect_type_variables(&parameter.ty, names);
            }
            collect_type_variables(ret, names);
            let _ = throws;
        }
        Ty::Future(value, error, _) => {
            collect_type_variables(value, names);
            collect_type_variables(error, names);
        }
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::Literal(..)
        | Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Void { .. }
        | Ty::TypeAlias(..)
        | Ty::BuiltinUnknown { .. }
        | Ty::Never { .. } => {}
    }
}

fn translate_inner(
    ty: &Ty,
    aliases: &AliasMap<'_>,
    type_variables: &BTreeMap<String, String>,
    active_aliases: &mut HashSet<Name>,
) -> TranslatedType {
    match ty {
        Ty::Int { .. } => TranslatedType::primitive("long"),
        Ty::Bigint { .. } => TranslatedType::primitive("global::System.Numerics.BigInteger"),
        Ty::Float { .. } => TranslatedType::primitive("double"),
        Ty::String { .. } => TranslatedType::primitive("string"),
        Ty::Bool { .. } => TranslatedType::primitive("bool"),
        Ty::Null { .. } => TranslatedType::primitive("object?"),
        Ty::Uint8Array { .. } => TranslatedType::primitive("byte[]"),
        Ty::Literal(literal, ..) => translate_literal(literal),
        Ty::Union(options, _) => translate_union(options, aliases, type_variables, active_aliases),
        Ty::BuiltinUnknown { .. } => TranslatedType::primitive("object?"),
        Ty::TypeVar(name, _) => TranslatedType::primitive(
            type_variables
                .get(name.as_str())
                .cloned()
                .unwrap_or_else(|| namespace_segment(name.as_str())),
        ),
        Ty::List(inner, _) => {
            let inner = translate_inner(inner, aliases, type_variables, active_aliases);
            TranslatedType {
                source: format!("global::System.Collections.Generic.List<{}>", inner.source),
                primitive_codec: inner.primitive_codec,
                async_callback_source: None,
                contains_host_callable: inner.contains_host_callable,
            }
        }
        Ty::Map { key, value, .. } => {
            let key = translate_map_key(key, aliases, type_variables, active_aliases);
            let value = translate_inner(value, aliases, type_variables, active_aliases);
            TranslatedType {
                source: format!(
                    "global::System.Collections.Generic.Dictionary<{}, {}>",
                    key.source, value.source
                ),
                primitive_codec: key.primitive_codec && value.primitive_codec,
                async_callback_source: None,
                contains_host_callable: key.contains_host_callable || value.contains_host_callable,
            }
        }
        Ty::Void { .. } | Ty::Never { .. } => TranslatedType::stub("object?"),
        Ty::Class(name, arguments, _) => {
            if !name.is_local()
                && name.to_string() != "baml.llm.Stream"
                && name.to_string() != "baml.stream.StreamFinished"
                && name.to_string() != "baml.llm.PromptAst"
                && name.to_string() != "baml.llm.PromptMessage"
                && name.to_string() != "baml.http.Request"
                && name.to_string() != "baml.http.Response"
                && name.to_string() != "baml.fs.File"
                && name.to_string() != "baml.http.SseStream"
                && name.to_string() != "baml.glob.Glob"
                && name.to_string() != "baml.glob.ScanOptions"
                && name.to_string() != "baml.spawn.CancelToken"
                && name.to_string() != "baml.spawn.TaskGroup"
                && name.to_string() != "baml.csv.CsvWriter"
                && name.to_string() != "baml.csv.CsvReader"
                && name.to_string() != "baml.csv.CsvRecord"
                && name.to_string() != "baml.csv.CsvPosition"
                && name.to_string() != "baml.csv.WriterOptions"
                && name.to_string() != "baml.csv.ReaderOptions"
                && name.to_string() != "baml.iter.Done"
                && name.to_string() != "baml.llm.Client"
                && name.to_string() != "baml.llm.RetryPolicy"
            {
                return TranslatedType::stub("object?");
            }
            let arguments = arguments
                .iter()
                .map(|argument| translate_inner(argument, aliases, type_variables, active_aliases))
                .collect::<Vec<_>>();
            let mut source = match name.to_string().as_str() {
                "baml.llm.Stream" if arguments.len() == 2 => {
                    format!(
                        "global::Baml.BamlStream<{}, {}>",
                        arguments[0].source, arguments[1].source
                    )
                }
                "baml.stream.StreamFinished" if arguments.is_empty() => {
                    "global::Baml.BamlStreamFinished".to_string()
                }
                "baml.llm.PromptAst" if arguments.is_empty() => {
                    "global::Baml.BamlPromptAst".to_string()
                }
                "baml.llm.PromptMessage" if arguments.is_empty() => {
                    "global::Baml.BamlPromptMessage".to_string()
                }
                "baml.http.Request" if arguments.is_empty() => {
                    "global::Baml.BamlHttpRequest".to_string()
                }
                "baml.http.Response" if arguments.is_empty() => {
                    "global::Baml.BamlHttpResponse".to_string()
                }
                "baml.fs.File" if arguments.is_empty() => "global::Baml.BamlFile".to_string(),
                "baml.http.SseStream" if arguments.is_empty() => {
                    "global::Baml.BamlSseStream".to_string()
                }
                "baml.glob.Glob" if arguments.is_empty() => "global::Baml.BamlGlob".to_string(),
                "baml.glob.ScanOptions" if arguments.is_empty() => {
                    "global::Baml.BamlGlobScanOptions".to_string()
                }
                "baml.spawn.CancelToken" if arguments.is_empty() => {
                    "global::Baml.BamlCancelToken".to_string()
                }
                "baml.spawn.TaskGroup" if arguments.is_empty() => {
                    "global::Baml.BamlTaskGroup".to_string()
                }
                "baml.csv.CsvWriter" if arguments.is_empty() => {
                    "global::Baml.BamlCsvWriter".to_string()
                }
                "baml.csv.CsvReader" if arguments.is_empty() => {
                    "global::Baml.BamlCsvReader".to_string()
                }
                "baml.csv.CsvRecord" if arguments.is_empty() => {
                    "global::Baml.BamlCsvRecord".to_string()
                }
                "baml.csv.CsvPosition" if arguments.is_empty() => {
                    "global::Baml.BamlCsvPosition".to_string()
                }
                "baml.csv.WriterOptions" if arguments.is_empty() => {
                    "global::Baml.BamlCsvWriterOptions".to_string()
                }
                "baml.csv.ReaderOptions" if arguments.is_empty() => {
                    "global::Baml.BamlCsvReaderOptions".to_string()
                }
                "baml.iter.Done" if arguments.is_empty() => {
                    "global::Baml.BamlIteratorDone".to_string()
                }
                "baml.llm.Client" if arguments.is_empty() => "global::Baml.BamlClient".to_string(),
                "baml.llm.RetryPolicy" if arguments.is_empty() => {
                    "global::Baml.BamlRetryPolicy".to_string()
                }
                _ => render_name(name, aliases),
            };
            if !arguments.is_empty() && name.to_string() != "baml.llm.Stream" {
                source.push('<');
                source.push_str(
                    &arguments
                        .iter()
                        .map(|argument| argument.source.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                source.push('>');
            }
            TranslatedType {
                source,
                primitive_codec: arguments.iter().all(|argument| argument.primitive_codec),
                async_callback_source: None,
                contains_host_callable: arguments
                    .iter()
                    .any(|argument| argument.contains_host_callable),
            }
        }
        Ty::Enum(name, _) | Ty::EnumVariant(name, _, _)
            if name.to_string() == "baml.llm.ClientType" =>
        {
            TranslatedType::primitive("global::Baml.BamlClientType")
        }
        Ty::Enum(name, _) | Ty::EnumVariant(name, _, _) => {
            TranslatedType::primitive(render_name(name, aliases))
        }
        Ty::Media(kind, _) => translate_media(*kind),
        Ty::RustType { .. } => TranslatedType::primitive("global::Baml.BamlHandle"),
        Ty::TypeAlias(name, _) => translate_alias(name, aliases, type_variables, active_aliases),
        Ty::Function {
            params,
            ret,
            throws,
            attr: _,
        } => {
            let _ = throws;
            translate_callable(params, ret, aliases, type_variables, active_aliases)
        }
        Ty::PromptAst { .. } => TranslatedType::primitive("global::Baml.BamlPromptAst"),
        Ty::Interface(..) | Ty::Future(..) | Ty::Type { .. } | Ty::Resource { .. } => {
            TranslatedType::stub("object?")
        }
    }
}

fn translate_callable(
    params: &[baml_codegen_types::CallableParam],
    ret: &Ty,
    aliases: &AliasMap<'_>,
    type_variables: &BTreeMap<String, String>,
    active_aliases: &mut HashSet<Name>,
) -> TranslatedType {
    let parameters = params
        .iter()
        .map(|parameter| translate_inner(&parameter.ty, aliases, type_variables, active_aliases))
        .collect::<Vec<_>>();
    let return_type = translate_inner(ret, aliases, type_variables, active_aliases);
    let supported = params.len() <= 16
        && params
            .iter()
            .all(|parameter| parameter.mode == CodegenFunctionParamMode::Required)
        && parameters.iter().all(|parameter| parameter.primitive_codec)
        && (matches!(ret, Ty::Void { .. }) || return_type.primitive_codec);
    if !supported {
        return TranslatedType {
            source: "global::System.Delegate".to_string(),
            primitive_codec: false,
            async_callback_source: None,
            contains_host_callable: true,
        };
    }

    let parameter_sources = parameters
        .iter()
        .map(|parameter| parameter.source.as_str())
        .collect::<Vec<_>>();
    let source = if matches!(ret, Ty::Void { .. }) {
        render_delegate("global::System.Action", &parameter_sources, None)
    } else {
        render_delegate(
            "global::System.Func",
            &parameter_sources,
            Some(return_type.source.as_str()),
        )
    };
    let async_return = if matches!(ret, Ty::Void { .. }) {
        "global::System.Threading.Tasks.ValueTask".to_string()
    } else {
        format!(
            "global::System.Threading.Tasks.ValueTask<{}>",
            return_type.source
        )
    };
    TranslatedType {
        source,
        primitive_codec: true,
        async_callback_source: Some(render_delegate(
            "global::System.Func",
            &parameter_sources,
            Some(&async_return),
        )),
        contains_host_callable: true,
    }
}

fn render_delegate(base: &str, parameters: &[&str], ret: Option<&str>) -> String {
    let mut arguments = parameters.to_vec();
    if let Some(ret) = ret {
        arguments.push(ret);
    }
    if arguments.is_empty() {
        base.to_string()
    } else {
        format!("{base}<{}>", arguments.join(", "))
    }
}

fn translate_alias(
    name: &Name,
    aliases: &AliasMap<'_>,
    type_variables: &BTreeMap<String, String>,
    active_aliases: &mut HashSet<Name>,
) -> TranslatedType {
    let Some(alias) = aliases.get(name) else {
        return TranslatedType::stub("object?");
    };
    if alias.recursive && !name.is_local() {
        return TranslatedType::primitive("object?");
    }
    if alias.recursive {
        return TranslatedType::primitive(render_name(name, aliases));
    }
    if !active_aliases.insert(name.clone()) {
        return TranslatedType::stub("object?");
    }

    let translated = translate_inner(&alias.resolves_to, aliases, type_variables, active_aliases);
    active_aliases.remove(name);
    translated
}

fn translate_media(kind: MediaKind) -> TranslatedType {
    match kind {
        MediaKind::Image => TranslatedType::primitive("global::Baml.BamlImage"),
        MediaKind::Audio => TranslatedType::primitive("global::Baml.BamlAudio"),
        MediaKind::Video => TranslatedType::primitive("global::Baml.BamlVideo"),
        MediaKind::Pdf => TranslatedType::primitive("global::Baml.BamlPdf"),
        MediaKind::Generic => TranslatedType::stub("object?"),
    }
}

fn render_name(name: &Name, aliases: &AliasMap<'_>) -> String {
    format!(
        "global::{}.{}",
        aliases.projected_namespace(name),
        aliases.projected_type_name(name)
    )
}

fn translate_literal(literal: &Literal) -> TranslatedType {
    match literal {
        Literal::Int(_) => TranslatedType::primitive("long"),
        Literal::Bigint(_) => TranslatedType::primitive("global::System.Numerics.BigInteger"),
        Literal::Float(_) => TranslatedType::primitive("double"),
        Literal::String(_) => TranslatedType::primitive("string"),
        Literal::Bool(_) => TranslatedType::primitive("bool"),
    }
}

fn translate_map_key(
    key: &Ty,
    aliases: &AliasMap<'_>,
    type_variables: &BTreeMap<String, String>,
    active_aliases: &mut HashSet<Name>,
) -> TranslatedType {
    if map_key_is_string_denoting(key, aliases, &mut HashSet::new()) {
        TranslatedType::primitive("string")
    } else {
        translate_inner(key, aliases, type_variables, active_aliases)
    }
}

fn map_key_is_string_denoting(
    key: &Ty,
    aliases: &AliasMap<'_>,
    active_aliases: &mut HashSet<Name>,
) -> bool {
    match key {
        Ty::String { .. } | Ty::Literal(Literal::String(_), ..) | Ty::Never { .. } => true,
        Ty::Union(members, _) => members
            .iter()
            .all(|member| map_key_is_string_denoting(member, aliases, active_aliases)),
        Ty::TypeAlias(name, _) => {
            if !active_aliases.insert(name.clone()) {
                return false;
            }
            let string_denoting = aliases.get(name).is_some_and(|alias| {
                map_key_is_string_denoting(&alias.resolves_to, aliases, active_aliases)
            });
            active_aliases.remove(name);
            string_denoting
        }
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::Literal(..)
        | Ty::Class(..)
        | Ty::Interface(..)
        | Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::List(..)
        | Ty::Map { .. }
        | Ty::Function { .. }
        | Ty::Future(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Void { .. }
        | Ty::TypeVar(..)
        | Ty::BuiltinUnknown { .. } => false,
    }
}

fn translate_union(
    options: &[Ty],
    aliases: &AliasMap<'_>,
    type_variables: &BTreeMap<String, String>,
    active_aliases: &mut HashSet<Name>,
) -> TranslatedType {
    let non_null: Vec<&Ty> = options
        .iter()
        .filter(|option| !matches!(option, Ty::Null { .. }))
        .collect();
    let has_null = non_null.len() != options.len();

    let mut translated = Vec::<TranslatedType>::new();
    for option in &non_null {
        let item = translate_inner(option, aliases, type_variables, active_aliases);
        if let Some(existing) = translated
            .iter_mut()
            .find(|existing| existing.source == item.source)
        {
            existing.primitive_codec &= item.primitive_codec;
            existing.contains_host_callable |= item.contains_host_callable;
        } else {
            translated.push(item);
        }
    }

    if translated.len() == 1 {
        let mut inner = translated.pop().expect("one translated union alternative");
        inner.async_callback_source = None;
        if has_null {
            inner.source = if non_null
                .iter()
                .all(|option| type_variable_denoting(option, aliases, &mut HashSet::new()))
            {
                format!("global::Baml.BamlNullable<{}>", inner.source)
            } else if inner.source.ends_with('?') {
                inner.source
            } else {
                format!("{}?", inner.source)
            };
        }
        return inner;
    }

    if !(2..=32).contains(&translated.len()) {
        return TranslatedType::stub("object?");
    }

    let union = format!(
        "global::Baml.BamlUnion<{}>",
        translated
            .iter()
            .map(|item| item.source.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    TranslatedType {
        source: if has_null { format!("{union}?") } else { union },
        primitive_codec: translated.iter().all(|item| item.primitive_codec),
        async_callback_source: None,
        contains_host_callable: translated.iter().any(|item| item.contains_host_callable),
    }
}

fn type_variable_denoting(
    ty: &Ty,
    aliases: &AliasMap<'_>,
    active_aliases: &mut HashSet<Name>,
) -> bool {
    match ty {
        Ty::TypeVar(..) => true,
        Ty::TypeAlias(name, _) => {
            if !active_aliases.insert(name.clone()) {
                return false;
            }
            let is_type_variable = aliases.get(name).is_some_and(|alias| {
                !alias.recursive
                    && type_variable_denoting(&alias.resolves_to, aliases, active_aliases)
            });
            active_aliases.remove(name);
            is_type_variable
        }
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::Literal(..)
        | Ty::Class(..)
        | Ty::Interface(..)
        | Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::List(..)
        | Ty::Map { .. }
        | Ty::Union(..)
        | Ty::Function { .. }
        | Ty::Future(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Void { .. }
        | Ty::BuiltinUnknown { .. }
        | Ty::Never { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use baml_base::{Name as BaseName, TyAttr};
    use baml_codegen_types::{CallableParam, Freshness, Origin};

    use super::*;

    fn without_aliases(ty: &Ty) -> TranslatedType {
        translate(ty, &AliasMap::new())
    }

    fn a() -> TyAttr {
        TyAttr::EMPTY
    }

    fn int() -> Ty {
        Ty::Int { attr: a() }
    }

    fn string() -> Ty {
        Ty::String { attr: a() }
    }

    fn bool_() -> Ty {
        Ty::Bool { attr: a() }
    }

    fn null() -> Ty {
        Ty::Null { attr: a() }
    }

    fn type_var(name: &str) -> Ty {
        Ty::TypeVar(BaseName::new(name), a())
    }

    fn list(inner: Ty) -> Ty {
        Ty::List(Box::new(inner), a())
    }

    fn union(members: Vec<Ty>) -> Ty {
        Ty::Union(members, a())
    }

    fn alias_ty(name: Name) -> Ty {
        Ty::TypeAlias(name, a())
    }

    fn class(name: Name, arguments: Vec<Ty>) -> Ty {
        Ty::Class(name, arguments, a())
    }

    fn enum_(name: Name) -> Ty {
        Ty::Enum(name, a())
    }

    fn literal(value: Literal) -> Ty {
        Ty::Literal(value, Freshness::Regular, a())
    }

    fn function(params: Vec<CallableParam>, ret: Ty) -> Ty {
        Ty::Function {
            params,
            ret: Box::new(ret),
            throws: Box::new(Ty::Never { attr: a() }),
            attr: a(),
        }
    }

    #[test]
    fn translates_basic_wire_types() {
        assert_eq!(without_aliases(&int()), TranslatedType::primitive("long"));
        assert_eq!(
            without_aliases(&string()),
            TranslatedType::primitive("string")
        );
        assert_eq!(
            without_aliases(&union(vec![string(), null()])),
            TranslatedType::primitive("string?")
        );
        assert_eq!(
            without_aliases(&union(vec![int(), string()])),
            TranslatedType::primitive("global::Baml.BamlUnion<long, string>")
        );
        assert_eq!(
            without_aliases(&union(vec![int(), null(), string()])),
            TranslatedType::primitive("global::Baml.BamlUnion<long, string>?")
        );
        assert_eq!(
            without_aliases(&union(vec![int(), string(), bool_()])),
            TranslatedType::primitive("global::Baml.BamlUnion<long, string, bool>")
        );
        assert_eq!(
            without_aliases(&union(vec![type_var("T"), null()])).source,
            "global::Baml.BamlNullable<T>"
        );
        assert_eq!(
            without_aliases(&list(int())),
            TranslatedType::primitive("global::System.Collections.Generic.List<long>")
        );
        assert_eq!(
            without_aliases(&Ty::Map {
                key: Box::new(string()),
                value: Box::new(union(vec![int(), null()])),
                attr: a(),
            }),
            TranslatedType::primitive(
                "global::System.Collections.Generic.Dictionary<string, long?>"
            )
        );
    }

    #[test]
    fn collapses_literal_unions_after_clr_widening() {
        assert_eq!(
            without_aliases(&union(vec![
                literal(Literal::String("a".to_string())),
                literal(Literal::String("b".to_string())),
            ])),
            TranslatedType::primitive("string")
        );
        assert_eq!(
            without_aliases(&union(vec![
                literal(Literal::Int(0x1)),
                literal(Literal::Int(0x2)),
            ])),
            TranslatedType::primitive("long")
        );
        assert_eq!(
            without_aliases(&union(vec![
                literal(Literal::Int(1)),
                literal(Literal::Int(2)),
                null(),
            ])),
            TranslatedType::primitive("long?")
        );

        let unsupported_collision = without_aliases(&union(vec![
            Ty::BuiltinUnknown { attr: a() },
            Ty::Interface(
                Name::local(BaseName::new("Shape")),
                Vec::new(),
                Vec::new(),
                a(),
            ),
        ]));
        assert_eq!(unsupported_collision.source, "object?");
        assert!(!unsupported_collision.primitive_codec);

        let callable_collision = without_aliases(&union(vec![
            function(
                vec![CallableParam {
                    name: None,
                    ty: string(),
                    mode: CodegenFunctionParamMode::Required,
                }],
                string(),
            ),
            function(
                vec![CallableParam {
                    name: None,
                    ty: literal(Literal::String("value".to_string())),
                    mode: CodegenFunctionParamMode::Required,
                }],
                string(),
            ),
        ]));
        assert_eq!(
            callable_collision.source,
            "global::System.Func<string, string>"
        );
        assert!(callable_collision.primitive_codec);
        assert!(callable_collision.contains_host_callable);
        assert_eq!(callable_collision.async_callback_source, None);
    }

    #[test]
    fn maps_alias_chains_of_string_literal_unions_to_string_keys() {
        let literals_name = Name::new(
            BaseName::new("user"),
            Vec::new(),
            BaseName::new("LiteralKey"),
        );
        let chain_name = Name::new(BaseName::new("user"), Vec::new(), BaseName::new("KeyChain"));
        let literals = TypeAlias {
            name: literals_name.clone(),
            resolves_to: union(vec![
                literal(Literal::String("a".to_string())),
                literal(Literal::String("b".to_string())),
            ]),
            recursive: false,
            origin: Origin {
                source_file_path: "keys.baml".to_string(),
                span_start: 0,
            },
        };
        let chain = TypeAlias {
            name: chain_name.clone(),
            resolves_to: alias_ty(literals_name.clone()),
            recursive: false,
            origin: Origin {
                source_file_path: "keys.baml".to_string(),
                span_start: 1,
            },
        };
        let mut aliases = AliasMap::new();
        aliases.insert(literals_name, &literals);
        aliases.insert(chain_name.clone(), &chain);

        assert_eq!(
            translate(
                &Ty::Map {
                    key: Box::new(alias_ty(chain_name)),
                    value: Box::new(int()),
                    attr: a(),
                },
                &aliases,
            ),
            TranslatedType::primitive(
                "global::System.Collections.Generic.Dictionary<string, long>"
            )
        );
    }

    #[test]
    fn preserves_other_compiler_valid_map_key_projections() {
        let enum_name = Name::new(
            BaseName::new("user"),
            vec![BaseName::new("models")],
            BaseName::new("Status"),
        );
        for (key, expected) in [
            (int(), "long".to_string()),
            (bool_(), "bool".to_string()),
            (
                Ty::Enum(enum_name.clone(), a()),
                "global::BamlSdk.Models.Status".to_string(),
            ),
        ] {
            let translated = without_aliases(&Ty::Map {
                key: Box::new(key),
                value: Box::new(string()),
                attr: a(),
            });
            assert_eq!(
                translated,
                TranslatedType::primitive(format!(
                    "global::System.Collections.Generic.Dictionary<{expected}, string>"
                ))
            );
        }
    }

    #[test]
    fn lowers_enum_variants_to_the_owning_enum() {
        let enum_name = Name::new(
            BaseName::new("user"),
            vec![BaseName::new("models")],
            BaseName::new("Status"),
        );

        assert_eq!(
            without_aliases(&Ty::EnumVariant(enum_name, BaseName::new("Ready"), a(),)),
            TranslatedType::primitive("global::BamlSdk.Models.Status")
        );
    }

    #[test]
    fn classifies_canonical_special_types_and_ignores_unchecked_throws() {
        assert_eq!(
            without_aliases(&Ty::PromptAst { attr: a() }),
            TranslatedType::primitive("global::Baml.BamlPromptAst")
        );
        for unsupported in [
            Ty::Interface(
                Name::local(BaseName::new("Shape")),
                Vec::new(),
                Vec::new(),
                a(),
            ),
            Ty::Future(Box::new(string()), Box::new(Ty::Never { attr: a() }), a()),
            Ty::Type { attr: a() },
            Ty::Resource { attr: a() },
            Ty::Void { attr: a() },
            Ty::Never { attr: a() },
        ] {
            assert_eq!(
                without_aliases(&unsupported),
                TranslatedType::stub("object?")
            );
        }

        let callable = Ty::Function {
            params: vec![CallableParam {
                name: None,
                ty: int(),
                mode: CodegenFunctionParamMode::Required,
            }],
            ret: Box::new(Ty::Void { attr: a() }),
            throws: Box::new(string()),
            attr: a(),
        };
        let translated = without_aliases(&callable);
        assert_eq!(translated.source, "global::System.Action<long>");
        assert_eq!(
            translated.async_callback_source.as_deref(),
            Some("global::System.Func<long, global::System.Threading.Tasks.ValueTask>")
        );
        assert!(translated.primitive_codec);
        assert!(translated.contains_host_callable);
    }

    #[test]
    fn projects_stream_type_names_from_canonical_name_accessors() {
        let stream = Name::local(BaseName::new("Payload$stream"));
        assert_eq!(preferred_type_name(&stream), "PayloadStream");
    }

    #[test]
    fn translates_recursive_vendor_aliases_as_dynamic_values() {
        let alias_name = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("json")],
            BaseName::new("json"),
        );
        let alias = TypeAlias {
            name: alias_name.clone(),
            resolves_to: union(vec![string(), list(alias_ty(alias_name.clone()))]),
            recursive: true,
            origin: Origin {
                source_file_path: "stdlib.baml".to_string(),
                span_start: 0,
            },
        };
        let aliases = [(alias_name.clone(), &alias)].into_iter().collect();

        assert_eq!(
            translate(&alias_ty(alias_name), &aliases),
            TranslatedType::primitive("object?")
        );
    }

    #[test]
    fn translates_required_host_callables_to_sync_and_async_delegates() {
        let translated = without_aliases(&function(
            vec![CallableParam {
                name: None,
                ty: int(),
                mode: CodegenFunctionParamMode::Required,
            }],
            string(),
        ));

        assert_eq!(translated.source, "global::System.Func<long, string>");
        assert_eq!(
            translated.async_callback_source.as_deref(),
            Some("global::System.Func<long, global::System.Threading.Tasks.ValueTask<string>>")
        );
        assert!(translated.primitive_codec);
        assert!(translated.contains_host_callable);

        let optional = without_aliases(&function(
            vec![CallableParam {
                name: Some(BaseName::new("value")),
                ty: int(),
                mode: CodegenFunctionParamMode::Optional,
            }],
            string(),
        ));
        assert!(!optional.primitive_codec);

        let optional_ty = function(
            vec![CallableParam {
                name: Some(BaseName::new("value")),
                ty: int(),
                mode: CodegenFunctionParamMode::Optional,
            }],
            string(),
        );
        let translated_optional = translate_argument(
            &optional_ty,
            &AliasMap::new(),
            "user.tests.call",
            "callback",
        );
        assert!(translated_optional.primitive_codec);
        assert!(translated_optional.source.starts_with("BamlCallback_"));
        let callback = callback_delegate(
            &optional_ty,
            &AliasMap::new(),
            "user.tests.call",
            "callback",
        )
        .unwrap();
        let mut rendered = String::new();
        callback.render(&mut rendered);
        assert!(rendered.contains("BamlOptional<long>"));
        assert!(rendered.contains("BamlWireNameAttribute(\"value\")"));

        let generic_optional_ty = function(
            vec![
                CallableParam {
                    name: Some(BaseName::new("value")),
                    ty: type_var("T"),
                    mode: CodegenFunctionParamMode::Required,
                },
                CallableParam {
                    name: Some(BaseName::new("fallback")),
                    ty: type_var("T"),
                    mode: CodegenFunctionParamMode::Optional,
                },
            ],
            type_var("T"),
        );
        let generic_callback = callback_delegate(
            &generic_optional_ty,
            &AliasMap::new(),
            "user.tests.call_generic",
            "callback",
        )
        .expect("generic optional callback should have a generated delegate");
        assert!(generic_callback.sync_name.ends_with("<T>"));
        assert!(generic_callback.async_name.ends_with("Async<T>"));
        let mut rendered = String::new();
        generic_callback.render(&mut rendered);
        assert!(rendered.contains("delegate T BamlCallback_"));
        assert!(rendered.contains("<T>("));
    }

    #[test]
    fn translates_builtin_stream_handles_to_the_managed_wrapper() {
        let stream_name = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("llm")],
            BaseName::new("Stream"),
        );
        let translated = without_aliases(&class(
            stream_name,
            vec![union(vec![null(), string()]), string()],
        ));

        assert_eq!(
            translated.source,
            "global::Baml.BamlStream<string?, string>"
        );
        assert!(translated.primitive_codec);
    }

    #[test]
    fn translates_builtin_stdlib_classes_to_managed_types() {
        let prompt_ast = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("llm")],
            BaseName::new("PromptAst"),
        );
        let prompt_message = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("llm")],
            BaseName::new("PromptMessage"),
        );
        let http_request = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("http")],
            BaseName::new("Request"),
        );
        let http_response = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("http")],
            BaseName::new("Response"),
        );
        let file = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("fs")],
            BaseName::new("File"),
        );
        let sse_stream = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("http")],
            BaseName::new("SseStream"),
        );
        let glob = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("glob")],
            BaseName::new("Glob"),
        );
        let glob_scan_options = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("glob")],
            BaseName::new("ScanOptions"),
        );
        let cancel_token = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("spawn")],
            BaseName::new("CancelToken"),
        );
        let task_group = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("spawn")],
            BaseName::new("TaskGroup"),
        );
        let csv_writer = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("csv")],
            BaseName::new("CsvWriter"),
        );
        let csv_reader = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("csv")],
            BaseName::new("CsvReader"),
        );
        let csv_record = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("csv")],
            BaseName::new("CsvRecord"),
        );
        let csv_position = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("csv")],
            BaseName::new("CsvPosition"),
        );
        let csv_writer_options = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("csv")],
            BaseName::new("WriterOptions"),
        );
        let csv_reader_options = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("csv")],
            BaseName::new("ReaderOptions"),
        );
        let client = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("llm")],
            BaseName::new("Client"),
        );
        let retry_policy = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("llm")],
            BaseName::new("RetryPolicy"),
        );
        let client_type = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("llm")],
            BaseName::new("ClientType"),
        );

        assert_eq!(
            without_aliases(&class(prompt_ast, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlPromptAst")
        );
        assert_eq!(
            without_aliases(&class(prompt_message, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlPromptMessage")
        );
        assert_eq!(
            without_aliases(&class(http_request, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlHttpRequest")
        );
        assert_eq!(
            without_aliases(&class(http_response, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlHttpResponse")
        );
        assert_eq!(
            without_aliases(&class(file, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlFile")
        );
        assert_eq!(
            without_aliases(&class(sse_stream, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlSseStream")
        );
        assert_eq!(
            without_aliases(&class(glob, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlGlob")
        );
        assert_eq!(
            without_aliases(&class(glob_scan_options, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlGlobScanOptions")
        );
        assert_eq!(
            without_aliases(&class(cancel_token, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCancelToken")
        );
        assert_eq!(
            without_aliases(&class(task_group, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlTaskGroup")
        );
        assert_eq!(
            without_aliases(&class(csv_writer, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCsvWriter")
        );
        assert_eq!(
            without_aliases(&class(csv_reader, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCsvReader")
        );
        assert_eq!(
            without_aliases(&class(csv_record, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCsvRecord")
        );
        assert_eq!(
            without_aliases(&class(csv_position, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCsvPosition")
        );
        assert_eq!(
            without_aliases(&class(csv_writer_options, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCsvWriterOptions")
        );
        assert_eq!(
            without_aliases(&class(csv_reader_options, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCsvReaderOptions")
        );
        assert_eq!(
            without_aliases(&class(client, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlClient")
        );
        assert_eq!(
            without_aliases(&class(retry_policy, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlRetryPolicy")
        );
        assert_eq!(
            without_aliases(&enum_(client_type)),
            TranslatedType::primitive("global::Baml.BamlClientType")
        );
    }

    #[test]
    fn flattens_non_recursive_aliases_and_names_recursive_aliases() {
        let alias_name = Name::new(BaseName::new("user"), Vec::new(), BaseName::new("ProbeId"));
        let alias = TypeAlias {
            name: alias_name.clone(),
            resolves_to: int(),
            recursive: false,
            origin: Origin {
                source_file_path: "main.baml".to_string(),
                span_start: 0,
            },
        };
        let mut aliases = AliasMap::new();
        aliases.insert(alias_name.clone(), &alias);

        assert_eq!(
            translate(&alias_ty(alias_name), &aliases),
            TranslatedType::primitive("long")
        );

        let recursive_name = Name::new(
            BaseName::new("user"),
            Vec::new(),
            BaseName::new("Recursive"),
        );
        let recursive = TypeAlias {
            name: recursive_name.clone(),
            resolves_to: list(alias_ty(recursive_name.clone())),
            recursive: true,
            origin: Origin {
                source_file_path: "main.baml".to_string(),
                span_start: 1,
            },
        };
        aliases.insert(recursive_name.clone(), &recursive);
        assert_eq!(
            translate(&alias_ty(recursive_name), &aliases),
            TranslatedType::primitive("global::BamlSdk.Recursive")
        );
    }
}
