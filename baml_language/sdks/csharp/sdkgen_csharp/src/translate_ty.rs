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
            .unwrap_or_else(|| namespace_segment(name.name.as_str()))
    }

    fn projected_namespace(&self, name: &Name) -> String {
        self.projected_namespaces
            .get(name)
            .cloned()
            .unwrap_or_else(|| route(name).namespace)
    }
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
    let Ty::Callable { params, ret } = ty else {
        return None;
    };
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
        || (!matches!(ret.as_ref(), Ty::Unit) && !translated_return.primitive_codec)
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
        returns_void: matches!(ret.as_ref(), Ty::Unit),
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
        Ty::TypeVar(name) => {
            names.insert(name.to_string());
        }
        Ty::List(inner) => collect_type_variables(inner, names),
        Ty::Map { key, value } => {
            collect_type_variables(key, names);
            collect_type_variables(value, names);
        }
        Ty::Union(options) => {
            for option in options {
                collect_type_variables(option, names);
            }
        }
        Ty::Class(_, arguments) => {
            for argument in arguments {
                collect_type_variables(argument, names);
            }
        }
        Ty::Callable { params, ret } => {
            for parameter in params {
                collect_type_variables(&parameter.ty, names);
            }
            collect_type_variables(ret, names);
        }
        _ => {}
    }
}

fn translate_inner(
    ty: &Ty,
    aliases: &AliasMap<'_>,
    type_variables: &BTreeMap<String, String>,
    active_aliases: &mut HashSet<Name>,
) -> TranslatedType {
    match ty {
        Ty::Int => TranslatedType::primitive("long"),
        Ty::Bigint => TranslatedType::primitive("global::System.Numerics.BigInteger"),
        Ty::Float => TranslatedType::primitive("double"),
        Ty::String => TranslatedType::primitive("string"),
        Ty::Bool => TranslatedType::primitive("bool"),
        Ty::Null => TranslatedType::primitive("object?"),
        Ty::Uint8Array => TranslatedType::primitive("byte[]"),
        Ty::Literal(literal) => translate_literal(literal),
        Ty::Union(options) => translate_union(options, aliases, type_variables, active_aliases),
        Ty::BuiltinUnknown => TranslatedType::primitive("object?"),
        Ty::TypeVar(name) => TranslatedType::primitive(
            type_variables
                .get(name.as_str())
                .cloned()
                .unwrap_or_else(|| namespace_segment(name.as_str())),
        ),
        Ty::List(inner) => {
            let inner = translate_inner(inner, aliases, type_variables, active_aliases);
            TranslatedType {
                source: format!("global::System.Collections.Generic.List<{}>", inner.source),
                primitive_codec: inner.primitive_codec,
                async_callback_source: None,
                contains_host_callable: inner.contains_host_callable,
            }
        }
        Ty::Map { key, value } => {
            let key = translate_inner(key, aliases, type_variables, active_aliases);
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
        Ty::Unit => TranslatedType::stub("object?"),
        Ty::Class(name, arguments) => {
            if name.pkg.as_str() != "user"
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
        Ty::Enum(name) if name.to_string() == "baml.llm.ClientType" => {
            TranslatedType::primitive("global::Baml.BamlClientType")
        }
        Ty::Enum(name) => TranslatedType::primitive(render_name(name, aliases)),
        Ty::Media(kind) => translate_media(*kind),
        Ty::RustType => TranslatedType::primitive("global::Baml.BamlHandle"),
        Ty::TypeAlias(name) => translate_alias(name, aliases, type_variables, active_aliases),
        Ty::Callable { params, ret } => {
            translate_callable(params, ret, aliases, type_variables, active_aliases)
        }
        Ty::BamlOptions => TranslatedType::primitive("object?"),
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
        && (matches!(ret, Ty::Unit) || return_type.primitive_codec);
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
    let source = if matches!(ret, Ty::Unit) {
        render_delegate("global::System.Action", &parameter_sources, None)
    } else {
        render_delegate(
            "global::System.Func",
            &parameter_sources,
            Some(return_type.source.as_str()),
        )
    };
    let async_return = if matches!(ret, Ty::Unit) {
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
    if alias.recursive && name.pkg.as_str() != "user" {
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

fn translate_union(
    options: &[Ty],
    aliases: &AliasMap<'_>,
    type_variables: &BTreeMap<String, String>,
    active_aliases: &mut HashSet<Name>,
) -> TranslatedType {
    let non_null: Vec<&Ty> = options
        .iter()
        .filter(|option| !matches!(option, Ty::Null))
        .collect();
    let has_null = non_null.len() != options.len();

    if has_null && non_null.len() == 1 {
        let inner = translate_inner(non_null[0], aliases, type_variables, active_aliases);
        let source = match non_null[0] {
            Ty::TypeVar(_) => format!("global::Baml.BamlNullable<{}>", inner.source),
            _ if inner.source.ends_with('?') => inner.source.clone(),
            _ => format!("{}?", inner.source),
        };
        return TranslatedType {
            source,
            primitive_codec: inner.primitive_codec,
            async_callback_source: None,
            contains_host_callable: inner.contains_host_callable,
        };
    }

    let translated = non_null
        .iter()
        .map(|option| translate_inner(option, aliases, type_variables, active_aliases))
        .collect::<Vec<_>>();
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

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::Origin;

    use super::*;

    fn without_aliases(ty: &Ty) -> TranslatedType {
        translate(ty, &AliasMap::new())
    }

    #[test]
    fn translates_basic_wire_types() {
        assert_eq!(without_aliases(&Ty::Int), TranslatedType::primitive("long"));
        assert_eq!(
            without_aliases(&Ty::String),
            TranslatedType::primitive("string")
        );
        assert_eq!(
            without_aliases(&Ty::Union(vec![Ty::String, Ty::Null])),
            TranslatedType::primitive("string?")
        );
        assert_eq!(
            without_aliases(&Ty::Union(vec![Ty::Int, Ty::String])),
            TranslatedType::primitive("global::Baml.BamlUnion<long, string>")
        );
        assert_eq!(
            without_aliases(&Ty::Union(vec![Ty::Int, Ty::Null, Ty::String])),
            TranslatedType::primitive("global::Baml.BamlUnion<long, string>?")
        );
        assert_eq!(
            without_aliases(&Ty::Union(vec![Ty::Int, Ty::String, Ty::Bool])),
            TranslatedType::primitive("global::Baml.BamlUnion<long, string, bool>")
        );
        assert_eq!(
            without_aliases(&Ty::Union(vec![Ty::TypeVar(BaseName::new("T")), Ty::Null,])).source,
            "global::Baml.BamlNullable<T>"
        );
        assert_eq!(
            without_aliases(&Ty::List(Box::new(Ty::Int))),
            TranslatedType::primitive("global::System.Collections.Generic.List<long>")
        );
        assert_eq!(
            without_aliases(&Ty::Map {
                key: Box::new(Ty::String),
                value: Box::new(Ty::Union(vec![Ty::Int, Ty::Null])),
            }),
            TranslatedType::primitive(
                "global::System.Collections.Generic.Dictionary<string, long?>"
            )
        );
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
            resolves_to: Ty::Union(vec![
                Ty::String,
                Ty::List(Box::new(Ty::TypeAlias(alias_name.clone()))),
            ]),
            recursive: true,
            origin: Origin {
                source_file_path: "stdlib.baml".to_string(),
                span_start: 0,
            },
        };
        let aliases = [(alias_name.clone(), &alias)].into_iter().collect();

        assert_eq!(
            translate(&Ty::TypeAlias(alias_name), &aliases),
            TranslatedType::primitive("object?")
        );
    }

    #[test]
    fn translates_required_host_callables_to_sync_and_async_delegates() {
        let translated = without_aliases(&Ty::Callable {
            params: vec![baml_codegen_types::CallableParam {
                name: None,
                ty: Ty::Int,
                mode: CodegenFunctionParamMode::Required,
            }],
            ret: Box::new(Ty::String),
        });

        assert_eq!(translated.source, "global::System.Func<long, string>");
        assert_eq!(
            translated.async_callback_source.as_deref(),
            Some("global::System.Func<long, global::System.Threading.Tasks.ValueTask<string>>")
        );
        assert!(translated.primitive_codec);
        assert!(translated.contains_host_callable);

        let optional = without_aliases(&Ty::Callable {
            params: vec![baml_codegen_types::CallableParam {
                name: Some(BaseName::new("value")),
                ty: Ty::Int,
                mode: CodegenFunctionParamMode::Optional,
            }],
            ret: Box::new(Ty::String),
        });
        assert!(!optional.primitive_codec);

        let optional_ty = Ty::Callable {
            params: vec![baml_codegen_types::CallableParam {
                name: Some(BaseName::new("value")),
                ty: Ty::Int,
                mode: CodegenFunctionParamMode::Optional,
            }],
            ret: Box::new(Ty::String),
        };
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

        let generic_optional_ty = Ty::Callable {
            params: vec![
                baml_codegen_types::CallableParam {
                    name: Some(BaseName::new("value")),
                    ty: Ty::TypeVar(BaseName::new("T")),
                    mode: CodegenFunctionParamMode::Required,
                },
                baml_codegen_types::CallableParam {
                    name: Some(BaseName::new("fallback")),
                    ty: Ty::TypeVar(BaseName::new("T")),
                    mode: CodegenFunctionParamMode::Optional,
                },
            ],
            ret: Box::new(Ty::TypeVar(BaseName::new("T"))),
        };
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
        let translated = without_aliases(&Ty::Class(
            stream_name,
            vec![Ty::Union(vec![Ty::Null, Ty::String]), Ty::String],
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
            without_aliases(&Ty::Class(prompt_ast, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlPromptAst")
        );
        assert_eq!(
            without_aliases(&Ty::Class(prompt_message, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlPromptMessage")
        );
        assert_eq!(
            without_aliases(&Ty::Class(http_request, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlHttpRequest")
        );
        assert_eq!(
            without_aliases(&Ty::Class(http_response, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlHttpResponse")
        );
        assert_eq!(
            without_aliases(&Ty::Class(file, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlFile")
        );
        assert_eq!(
            without_aliases(&Ty::Class(sse_stream, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlSseStream")
        );
        assert_eq!(
            without_aliases(&Ty::Class(glob, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlGlob")
        );
        assert_eq!(
            without_aliases(&Ty::Class(glob_scan_options, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlGlobScanOptions")
        );
        assert_eq!(
            without_aliases(&Ty::Class(cancel_token, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCancelToken")
        );
        assert_eq!(
            without_aliases(&Ty::Class(task_group, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlTaskGroup")
        );
        assert_eq!(
            without_aliases(&Ty::Class(csv_writer, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCsvWriter")
        );
        assert_eq!(
            without_aliases(&Ty::Class(csv_reader, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCsvReader")
        );
        assert_eq!(
            without_aliases(&Ty::Class(csv_record, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCsvRecord")
        );
        assert_eq!(
            without_aliases(&Ty::Class(csv_position, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCsvPosition")
        );
        assert_eq!(
            without_aliases(&Ty::Class(csv_writer_options, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCsvWriterOptions")
        );
        assert_eq!(
            without_aliases(&Ty::Class(csv_reader_options, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlCsvReaderOptions")
        );
        assert_eq!(
            without_aliases(&Ty::Class(client, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlClient")
        );
        assert_eq!(
            without_aliases(&Ty::Class(retry_policy, Vec::new())),
            TranslatedType::primitive("global::Baml.BamlRetryPolicy")
        );
        assert_eq!(
            without_aliases(&Ty::Enum(client_type)),
            TranslatedType::primitive("global::Baml.BamlClientType")
        );
    }

    #[test]
    fn flattens_non_recursive_aliases_and_names_recursive_aliases() {
        let alias_name = Name::new(BaseName::new("user"), Vec::new(), BaseName::new("ProbeId"));
        let alias = TypeAlias {
            name: alias_name.clone(),
            resolves_to: Ty::Int,
            recursive: false,
            origin: Origin {
                source_file_path: "main.baml".to_string(),
                span_start: 0,
            },
        };
        let mut aliases = AliasMap::new();
        aliases.insert(alias_name.clone(), &alias);

        assert_eq!(
            translate(&Ty::TypeAlias(alias_name), &aliases),
            TranslatedType::primitive("long")
        );

        let recursive_name = Name::new(
            BaseName::new("user"),
            Vec::new(),
            BaseName::new("Recursive"),
        );
        let recursive = TypeAlias {
            name: recursive_name.clone(),
            resolves_to: Ty::List(Box::new(Ty::TypeAlias(recursive_name.clone()))),
            recursive: true,
            origin: Origin {
                source_file_path: "main.baml".to_string(),
                span_start: 1,
            },
        };
        aliases.insert(recursive_name.clone(), &recursive);
        assert_eq!(
            translate(&Ty::TypeAlias(recursive_name), &aliases),
            TranslatedType::primitive("global::BamlSdk.Recursive")
        );
    }
}
