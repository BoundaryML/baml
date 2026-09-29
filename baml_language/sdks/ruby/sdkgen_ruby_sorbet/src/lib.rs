//! Ruby/Sorbet declarations with synchronous function dispatch.
use baml_sdkgen_types::{Name, NamingConvention, Symbol, SymbolPool, Ty};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, btree_map::Entry},
    fmt::Write as _,
    path::PathBuf,
};

/// Panics when two BAML declarations map to one Ruby name.
pub fn to_source_code_with_bytecode(
    pool: &SymbolPool,
    bytecode: &[u8],
    naming: NamingConvention,
) -> HashMap<PathBuf, String> {
    to_source_code_with_bytecode_and_skipped(pool, bytecode, naming)
        .unwrap_or_else(|error| panic!("{error}"))
        .0
}

/// Ruby names claimed per scope (module, class) so two BAML declarations
/// cannot silently overwrite each other after case folding or prefixing.
#[derive(Default)]
struct NameTable(BTreeMap<(String, String), String>);

impl NameTable {
    fn claim(&mut self, scope: &str, ruby: &str, baml: &str) -> Result<(), String> {
        match self.0.entry((scope.to_owned(), ruby.to_owned())) {
            Entry::Vacant(slot) => {
                slot.insert(baml.to_owned());
                Ok(())
            }
            Entry::Occupied(slot) if slot.get() == baml => Ok(()),
            Entry::Occupied(slot) => Err(format!(
                "Ruby name `{ruby}` in {scope} is generated for both BAML `{}` and `{baml}`; rename one",
                slot.get()
            )),
        }
    }
}

/// Also returns sorted BAML names omitted from the generated surface.
/// Fails when generated Ruby names collide, naming both BAML declarations.
pub fn to_source_code_with_bytecode_and_skipped(
    pool: &SymbolPool,
    bytecode: &[u8],
    naming: NamingConvention,
) -> Result<(HashMap<PathBuf, String>, Vec<String>), String> {
    let symbols: BTreeMap<_, _> = pool.iter().filter(|(n, _)| n.is_local()).collect();
    let mut supported: BTreeSet<Name> = symbols
        .iter()
        .filter_map(|(n, s)| match s {
            Symbol::Class(c) if c.generic_params.is_empty() => Some((*n).clone()),
            Symbol::Enum(_) => Some((*n).clone()),
            _ => None,
        })
        .collect();
    // Remove declarations referring to omitted types, including transitive edges.
    loop {
        let rejected: Vec<_> = supported
            .iter()
            .filter(|n| match pool.get(*n) {
                Some(Symbol::Class(c)) => c
                    .properties
                    .iter()
                    .any(|p| ruby_type(&p.ty, &supported).is_none()),
                _ => false,
            })
            .cloned()
            .collect();
        if rejected.is_empty() {
            break;
        }
        for n in rejected {
            supported.remove(&n);
        }
    }
    let mut skipped = BTreeSet::new();
    let mut namespaces: BTreeMap<String, String> = BTreeMap::new();
    let mut root = String::from(
        "# frozen_string_literal: true\nrequire \"baml/bridge\"\nrequire \"sorbet-runtime\"\nrequire_relative \"baml_sdk/bytecode\"\nmodule BamlSdk\n  def self.initialize!\n    Baml::Bridge.initialize!(BYTECODE)\n  end\nend\n",
    );
    let mut methods = Vec::new();
    let mut registry = String::new();
    let mut names = NameTable::default();
    for (name, symbol) in &symbols {
        let module = module_name(name);
        namespaces.entry(module.clone()).or_default();
        let segments: Vec<String> = name
            .namespace()
            .iter()
            .map(|n| constant(n.as_str()))
            .collect();
        for (depth, segment) in segments.iter().enumerate() {
            let scope = std::iter::once("BamlSdk")
                .chain(segments[..depth].iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("::");
            let namespace = name.namespace()[..=depth]
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(".");
            names.claim(&scope, segment, &format!("namespace {namespace}"))?;
        }
        if let Symbol::Class(c) = symbol {
            for method in c.static_methods.iter().chain(&c.instance_methods) {
                skipped.insert(format!("{}.{}", name, method.name));
            }
        }
        let body = namespaces.get_mut(&module).unwrap();
        match symbol {
            Symbol::Class(c) if supported.contains(*name) => {
                let fields = c
                    .properties
                    .iter()
                    .map(|p| {
                        format!(
                            "{:?} => :{}",
                            p.name.as_str(),
                            identifier(p.name.as_str(), naming)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(
                    registry,
                    "::Baml::Bridge.register_type({:?}, ::{}, fields: {{{fields}}})",
                    name.to_string(),
                    qualified(name)
                )
                .unwrap();
                names.claim(&module, &constant(name.name().as_str()), &name.to_string())?;
                writeln!(root, "class {} < T::Struct; end", qualified(name)).unwrap();
                writeln!(body, "class {}", constant(name.name().as_str())).unwrap();
                for p in &c.properties {
                    let field = identifier(p.name.as_str(), naming);
                    names.claim(
                        &qualified(name),
                        &field,
                        &format!("{name}.{}", p.name.as_str()),
                    )?;
                    writeln!(
                        body,
                        "  const :{}, {}",
                        field,
                        ruby_type(&p.ty, &supported).unwrap()
                    )
                    .unwrap();
                }
                body.push_str("end\n");
            }
            Symbol::Enum(e) if supported.contains(*name) => {
                writeln!(
                    registry,
                    "::Baml::Bridge.register_type({:?}, ::{})",
                    name.to_string(),
                    qualified(name)
                )
                .unwrap();
                names.claim(&module, &constant(name.name().as_str()), &name.to_string())?;
                writeln!(root, "class {} < T::Enum; end", qualified(name)).unwrap();
                writeln!(body, "class {}\n  enums do", constant(name.name().as_str())).unwrap();
                for v in &e.variants {
                    names.claim(
                        &qualified(name),
                        &constant(v.name.as_str()),
                        &format!("{name}.{}", v.name.as_str()),
                    )?;
                    writeln!(
                        body,
                        "    {} = new({:?})",
                        constant(v.name.as_str()),
                        v.value
                    )
                    .unwrap();
                }
                body.push_str("  end\nend\n");
            }
            Symbol::Function(f)
                if !name.name().as_str().contains('$')
                    && f.generic_params.is_empty()
                    && f.watchers.is_empty()
                    && ruby_type(&f.return_type, &supported).is_some()
                    && f.arguments
                        .iter()
                        .filter(|a| !omit_injected_argument(a, &supported))
                        .all(|a| ruby_type(&a.ty, &supported).is_some()) =>
            {
                let mut params = Vec::new();
                let mut required = Vec::new();
                let mut keywords = Vec::new();
                let mut arguments = Vec::new();
                for a in f
                    .arguments
                    .iter()
                    .filter(|a| !omit_injected_argument(a, &supported))
                {
                    let arg = identifier(a.name.as_str(), naming);
                    let ty = ruby_type(&a.ty, &supported).unwrap();
                    arguments.push(format!("{:?} => {arg}", a.name.as_str()));
                    // Omission/default handling is deferred.
                    if a.default.is_some() {
                        keywords.push(format!(
                            "{arg}: (raise ::Baml::Bridge::UnsupportedTypeError, {:?})",
                            format!("BAML argument default for {name}.{arg} is not yet supported")
                        ));
                    } else if matches!(&a.ty, Ty::Union(items) if items.iter().any(|t| matches!(t, Ty::Null)))
                    {
                        keywords.push(format!("{arg}: nil"));
                    } else {
                        required.push(arg.clone());
                    }
                    params.push(format!("{arg}: {ty}"));
                }
                required.extend(keywords);
                let method = identifier(f.name.as_str(), naming);
                names.claim(&module, &method, &name.to_string())?;
                let params = if params.is_empty() {
                    String::new()
                } else {
                    format!("params({}).", params.join(", "))
                };
                writeln!(body, "sig {{ {params}returns({}) }}\ndef self.{method}({})\n  ::Baml::Bridge.call(::BamlSdk::BYTECODE, {:?}, {{{}}})\nend", ruby_type(&f.return_type, &supported).unwrap(), required.join(", "), name.to_string(), arguments.join(", ")).unwrap();
                methods.push(format!(
                    "T::Utils.signature_for_method({module}.method(:{method}))\n"
                ));
            }
            _ => {
                skipped.insert(name.to_string());
            }
        }
    }
    // Define all namespace modules before the type shells above.
    let mut modules = BTreeSet::new();
    for module in namespaces.keys() {
        let parts: Vec<_> = module.split("::").collect();
        for end in 2..=parts.len() {
            modules.insert(parts[..end].join("::"));
        }
    }
    let mut declarations = String::new();
    for module in modules {
        writeln!(declarations, "module {module}; end").unwrap();
    }
    let position = root.find("class ").unwrap_or(root.len());
    root.insert_str(position, &declarations);
    let mut files = HashMap::new();
    for (module, body) in namespaces {
        let path = format!(
            "baml_sdk/{}.rb",
            module
                .strip_prefix("BamlSdk::")
                .unwrap_or("root")
                .replace("::", "/")
                .to_lowercase()
        );
        let body = body.lines().fold(String::new(), |mut out, line| {
            writeln!(out, "  {line}").unwrap();
            out
        });
        files.insert(
            PathBuf::from(&path),
            format!("# frozen_string_literal: true\nmodule {module}\n  extend T::Sig\n{body}end\n"),
        );
        writeln!(
            root,
            "require_relative {:?}",
            path.strip_suffix(".rb").unwrap()
        )
        .unwrap();
    }
    root.push_str(&registry);
    for method in methods {
        root.push_str(&method);
    }
    files.insert(PathBuf::from("baml_sdk.rb"), root);
    let bytes = baml_sdkgen_types::embedded_bytecode_base64(bytecode);
    files.insert(PathBuf::from("baml_sdk/bytecode.rb"), format!("# frozen_string_literal: true\nmodule BamlSdk\n  BYTECODE = \"{bytes}\".b.freeze\nend\n"));
    Ok((files, skipped.into_iter().collect()))
}

// Leave unsupported compiler-injected options to the engine's default, without
// relaxing the supported-type requirement for user-declared arguments.
fn omit_injected_argument(
    argument: &baml_sdkgen_types::FunctionArgument,
    supported: &BTreeSet<Name>,
) -> bool {
    argument.injected && argument.default.is_some() && ruby_type(&argument.ty, supported).is_none()
}

fn module_name(name: &Name) -> String {
    std::iter::once("BamlSdk".to_owned())
        .chain(name.namespace().iter().map(|n| constant(n.as_str())))
        .collect::<Vec<_>>()
        .join("::")
}
fn qualified(name: &Name) -> String {
    format!("{}::{}", module_name(name), constant(name.name().as_str()))
}
fn constant(name: &str) -> String {
    let mut capitalize = true;
    let mut result = String::new();
    for c in name.chars() {
        if c == '_' {
            capitalize = true;
        } else if capitalize {
            result.extend(c.to_uppercase());
            capitalize = false;
        } else {
            result.push(c);
        }
    }
    if name.starts_with('_') {
        format!("Baml{result}")
    } else {
        result
    }
}
fn identifier(name: &str, naming: NamingConvention) -> String {
    let mut result = String::new();
    let chars: Vec<_> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if naming == NamingConvention::Language && c.is_uppercase() {
            if i > 0
                && chars[i - 1] != '_'
                && (chars[i - 1].is_lowercase()
                    || chars.get(i + 1).is_some_and(|c| c.is_lowercase()))
            {
                result.push('_');
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(*c);
        }
    }
    if [
        "BEGIN",
        "END",
        "alias",
        "and",
        "begin",
        "break",
        "case",
        "class",
        "def",
        "defined",
        "do",
        "else",
        "elsif",
        "end",
        "ensure",
        "false",
        "for",
        "if",
        "in",
        "module",
        "next",
        "nil",
        "not",
        "or",
        "redo",
        "rescue",
        "retry",
        "return",
        "self",
        "super",
        "then",
        "true",
        "undef",
        "unless",
        "until",
        "when",
        "while",
        "yield",
        "__FILE__",
        "__LINE__",
        "__ENCODING__",
        // Core Object/Module/T::Sig/T::Struct methods that generated code or
        // Sorbet calls on the module or instance; shadowing them breaks loading.
        "method",
        "methods",
        "new",
        "send",
        "public_send",
        "object_id",
        "hash",
        "freeze",
        "dup",
        "clone",
        "inspect",
        "to_s",
        "to_h",
        "initialize",
        "instance_variable_get",
        "instance_variable_set",
        "define_method",
        "const_get",
        "extend",
        "include",
        "sig",
        "props",
        "serialize",
        "deserialize",
    ]
    .contains(&result.as_str())
    {
        format!("baml_{result}")
    } else {
        result
    }
}
fn ruby_type(ty: &Ty, supported: &BTreeSet<Name>) -> Option<String> {
    Some(match ty {
        Ty::Literal(baml_base::Literal::String(_), ..) => "String".into(),
        Ty::Literal(baml_base::Literal::Int(_), ..) => "Integer".into(),
        Ty::Literal(baml_base::Literal::Float(_), ..) => "Float".into(),
        Ty::Literal(baml_base::Literal::Bool(_), ..) => "T::Boolean".into(),
        Ty::String => "String".into(),
        Ty::Int => "Integer".into(),
        Ty::Float => "Float".into(),
        Ty::Bool => "T::Boolean".into(),
        Ty::Null => "NilClass".into(),
        Ty::Never => "T.noreturn".into(),
        Ty::List(t) => format!("T::Array[{}]", ruby_type(t, supported)?),
        Ty::Map { key, value } => format!(
            "T::Hash[{}, {}]",
            ruby_type(key, supported)?,
            ruby_type(value, supported)?
        ),
        Ty::Class(n, args) if args.is_empty() && supported.contains(n) => {
            format!("::{}", qualified(n))
        }
        Ty::Enum(n) if supported.contains(n) => format!("::{}", qualified(n)),
        Ty::Union(items) if items.len() == 2 && items.iter().any(|t| matches!(t, Ty::Null)) => {
            format!(
                "T.nilable({})",
                ruby_type(items.iter().find(|t| !matches!(t, Ty::Null))?, supported)?
            )
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruby_names_and_container_types() {
        assert_eq!(
            identifier("class", NamingConvention::Language),
            "baml_class"
        );
        assert_eq!(
            identifier("HTTPCall", NamingConvention::Language),
            "http_call"
        );
        assert_eq!(
            identifier("SomeCall", NamingConvention::PreserveCase),
            "SomeCall"
        );
        assert_eq!(constant("throws_test"), "ThrowsTest");
        assert_eq!(
            identifier("method", NamingConvention::Language),
            "baml_method"
        );
        assert_eq!(identifier("sig", NamingConvention::Language), "baml_sig");
        let mut names = NameTable::default();
        names
            .claim("BamlSdk", "hello_world", "user.HelloWorld")
            .unwrap();
        names
            .claim("BamlSdk", "hello_world", "user.HelloWorld")
            .unwrap();
        let error = names
            .claim("BamlSdk", "hello_world", "user.hello_world")
            .unwrap_err();
        assert!(
            error.contains("user.HelloWorld") && error.contains("user.hello_world"),
            "{error}"
        );
        names
            .claim("BamlSdk::Other", "hello_world", "user.other.hello_world")
            .unwrap();
        assert_eq!(constant("StreamE2ECollectResult"), "StreamE2ECollectResult");
        assert_eq!(constant("_private"), "BamlPrivate");
        assert_eq!(
            ruby_type(&Ty::Never, &BTreeSet::new()).as_deref(),
            Some("T.noreturn")
        );
        let string = Ty::String;
        let optional = Ty::Union(Box::new([string.clone(), Ty::Null]));
        let map = Ty::Map {
            key: Box::new(string),
            value: Box::new(Ty::List(Box::new(optional))),
        };
        assert_eq!(
            ruby_type(&map, &BTreeSet::new()).as_deref(),
            Some("T::Hash[String, T::Array[T.nilable(String)]]")
        );
        let unsupported = Ty::Union(Box::new([Ty::Int, Ty::Bool]));
        assert_eq!(ruby_type(&unsupported, &BTreeSet::new()), None);
    }

    #[test]
    fn unsupported_injected_defaults_are_omitted_without_dropping_user_arguments() {
        use baml_sdkgen_types::{Function, FunctionArgument, FunctionArgumentDefault, Origin};

        let function = Function {
            name: "extract".into(),
            generic_params: vec![],
            docstring: None,
            arguments: vec![
                FunctionArgument {
                    name: "text".into(),
                    docstring: None,
                    ty: Ty::String,
                    default: None,
                    injected: false,
                },
                FunctionArgument {
                    // Deliberately not named on_event: only the marker matters.
                    name: "callback".into(),
                    docstring: None,
                    ty: Ty::Function {
                        params: Box::new([]),
                        ret: Box::new(Ty::Null),
                        throws: Box::new(Ty::Never),
                    },
                    default: Some(FunctionArgumentDefault::Null),
                    injected: true,
                },
            ],
            return_type: Ty::String,
            throws: None,
            watchers: vec![],
            origin: Origin {
                source_file_path: "test.baml".into(),
                span_start: 0,
            },
        };
        let name = Name::new("user".into(), vec![], "extract".into());
        let generate = |function| {
            to_source_code_with_bytecode_and_skipped(
                &SymbolPool::from([(name.clone(), Symbol::Function(function))]),
                &[],
                NamingConvention::Language,
            )
            .unwrap()
        };
        let (files, skipped) = generate(function.clone());
        let source = &files[&PathBuf::from("baml_sdk/root.rb")];
        assert!(skipped.is_empty());
        assert!(
            source.contains("sig { params(text: String).returns(String) }"),
            "{source}"
        );
        assert!(source.contains("def self.extract(text)"), "{source}");
        assert!(
            source.contains(
                "::Baml::Bridge.call(::BamlSdk::BYTECODE, \"user.extract\", {\"text\" => text})"
            ),
            "{source}"
        );
        assert!(!source.contains("callback"), "{source}");

        let mut user_callback = function.clone();
        user_callback.arguments[1].injected = false;
        user_callback.arguments[1].name = "on_event".into();
        let (files, skipped) = generate(user_callback);
        assert_eq!(skipped, ["user.extract"]);
        assert!(!files[&PathBuf::from("baml_sdk/root.rb")].contains("def self.extract"));

        let mut required_callback = function.clone();
        required_callback.arguments[1].default = None;
        assert_eq!(generate(required_callback).1, ["user.extract"]);

        // Supported defaults keep the existing unsupported-omission behavior.
        for injected in [false, true] {
            let mut supported_default = function.clone();
            supported_default.arguments[1].injected = injected;
            supported_default.arguments[1].ty = Ty::String;
            let (files, skipped) = generate(supported_default);
            let source = &files[&PathBuf::from("baml_sdk/root.rb")];
            assert!(skipped.is_empty());
            assert!(source.contains("callback: (raise ::Baml::Bridge::UnsupportedTypeError"));
            assert!(source.contains("\"callback\" => callback"));
        }
    }

    #[test]
    fn bytecode_is_binary_and_generation_is_stable() {
        let first = to_source_code_with_bytecode(
            &SymbolPool::new(),
            &[0, 255, 34, 92],
            NamingConvention::Language,
        );
        let second = to_source_code_with_bytecode(
            &SymbolPool::new(),
            &[0, 255, 34, 92],
            NamingConvention::Language,
        );
        assert_eq!(first, second);
        let embedded = baml_sdkgen_types::embedded_bytecode_base64(&[0, 255, 34, 92]);
        assert_eq!(
            first[&PathBuf::from("baml_sdk/bytecode.rb")],
            format!(
                "# frozen_string_literal: true\nmodule BamlSdk\n  BYTECODE = \"{embedded}\".b.freeze\nend\n"
            )
        );
        assert!(embedded.is_ascii() && !embedded.contains(['\n', '\r']));
        assert!(
            first[&PathBuf::from("baml_sdk.rb")].contains("Baml::Bridge.initialize!(BYTECODE)")
        );
    }
}
