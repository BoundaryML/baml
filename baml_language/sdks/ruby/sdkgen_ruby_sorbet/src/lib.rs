//! Minimal Ruby/Sorbet declarations. Function dispatch is intentionally deferred.
use baml_sdkgen_types::{Name, NamingConvention, Symbol, SymbolPool, Ty};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    path::PathBuf,
};

pub fn to_source_code_with_bytecode(
    pool: &SymbolPool,
    bytecode: &[u8],
    naming: NamingConvention,
) -> HashMap<PathBuf, String> {
    to_source_code_with_bytecode_and_skipped(pool, bytecode, naming).0
}

/// Also returns sorted BAML names omitted from the generated surface.
pub fn to_source_code_with_bytecode_and_skipped(
    pool: &SymbolPool,
    bytecode: &[u8],
    naming: NamingConvention,
) -> (HashMap<PathBuf, String>, Vec<String>) {
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
    for (name, symbol) in &symbols {
        let module = module_name(name);
        namespaces.entry(module.clone()).or_default();
        if let Symbol::Class(c) = symbol {
            for method in c.static_methods.iter().chain(&c.instance_methods) {
                skipped.insert(format!("{}.{}", name, method.name));
            }
        }
        let body = namespaces.get_mut(&module).unwrap();
        match symbol {
            Symbol::Class(c) if supported.contains(*name) => {
                writeln!(root, "class {} < T::Struct; end", qualified(name)).unwrap();
                writeln!(body, "class {}", constant(name.name().as_str())).unwrap();
                for p in &c.properties {
                    writeln!(
                        body,
                        "  const :{}, {}",
                        identifier(p.name.as_str(), naming),
                        ruby_type(&p.ty, &supported).unwrap()
                    )
                    .unwrap();
                }
                body.push_str("end\n");
            }
            Symbol::Enum(e) if supported.contains(*name) => {
                writeln!(root, "class {} < T::Enum; end", qualified(name)).unwrap();
                writeln!(body, "class {}\n  enums do", constant(name.name().as_str())).unwrap();
                for v in &e.variants {
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
                        .all(|a| ruby_type(&a.ty, &supported).is_some()) =>
            {
                let mut params = Vec::new();
                let mut required = Vec::new();
                let mut keywords = Vec::new();
                for a in &f.arguments {
                    let arg = identifier(a.name.as_str(), naming);
                    let ty = ruby_type(&a.ty, &supported).unwrap();
                    // Default expressions are not executed in this dispatch-free scaffold.
                    // Raising as the Ruby default keeps the evaluated sig's type honest.
                    if a.default.is_some() {
                        keywords.push(format!(
                            "{arg}: (raise NotImplementedError, {:?})",
                            name.to_string()
                        ));
                    } else if matches!(&a.ty, Ty::Union(items) if items.iter().any(|t| matches!(t, Ty::Null { .. })))
                    {
                        keywords.push(format!("{arg}: nil"));
                    } else {
                        required.push(arg.clone());
                    }
                    params.push(format!("{arg}: {ty}"));
                }
                required.extend(keywords);
                let method = identifier(f.name.as_str(), naming);
                let params = if params.is_empty() {
                    String::new()
                } else {
                    format!("params({}).", params.join(", "))
                };
                writeln!(body, "sig {{ {params}returns({}) }}\ndef self.{method}({})\n  raise NotImplementedError, {:?}\nend", ruby_type(&f.return_type, &supported).unwrap(), required.join(", "), name.to_string()).unwrap();
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
    for method in methods {
        root.push_str(&method);
    }
    files.insert(PathBuf::from("baml_sdk.rb"), root);
    let bytes = baml_sdkgen_types::embedded_bytecode_base64(bytecode);
    files.insert(PathBuf::from("baml_sdk/bytecode.rb"), format!("# frozen_string_literal: true\nmodule BamlSdk\n  BYTECODE = \"{bytes}\".b.freeze\nend\n"));
    (files, skipped.into_iter().collect())
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
            result.push(c);
            capitalize = true;
        } else if capitalize {
            result.extend(c.to_uppercase());
            capitalize = false;
        } else {
            result.push(c);
        }
    }
    if result.starts_with('_') {
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
        Ty::String { .. } => "String".into(),
        Ty::Int { .. } => "Integer".into(),
        Ty::Float { .. } => "Float".into(),
        Ty::Bool { .. } => "T::Boolean".into(),
        Ty::Null { .. } => "NilClass".into(),
        Ty::List(t) => format!("T::Array[{}]", ruby_type(t, supported)?),
        Ty::Map { key, value, .. } => format!(
            "T::Hash[{}, {}]",
            ruby_type(key, supported)?,
            ruby_type(value, supported)?
        ),
        Ty::Class(n, args) if args.is_empty() && supported.contains(n) => {
            format!("::{}", qualified(n))
        }
        Ty::Enum(n) if supported.contains(n) => format!("::{}", qualified(n)),
        Ty::Union(items)
            if items.len() == 2 && items.iter().any(|t| matches!(t, Ty::Null { .. })) =>
        {
            format!(
                "T.nilable({})",
                ruby_type(
                    items.iter().find(|t| !matches!(t, Ty::Null { .. }))?,
                    supported
                )?
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
            format!("# frozen_string_literal: true\nmodule BamlSdk\n  BYTECODE = \"{embedded}\".b.freeze\nend\n")
        );
        assert!(embedded.is_ascii() && !embedded.contains(['\n', '\r']));
        assert!(
            first[&PathBuf::from("baml_sdk.rb")].contains("Baml::Bridge.initialize!(BYTECODE)")
        );
    }
}
