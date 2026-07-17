use std::collections::{BTreeMap, BTreeSet};

const KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "base",
    "bool",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "checked",
    "class",
    "const",
    "continue",
    "decimal",
    "default",
    "delegate",
    "do",
    "double",
    "else",
    "enum",
    "event",
    "explicit",
    "extern",
    "false",
    "finally",
    "fixed",
    "float",
    "for",
    "foreach",
    "goto",
    "if",
    "implicit",
    "in",
    "int",
    "interface",
    "internal",
    "is",
    "lock",
    "long",
    "namespace",
    "new",
    "null",
    "object",
    "operator",
    "out",
    "override",
    "params",
    "private",
    "protected",
    "public",
    "readonly",
    "ref",
    "return",
    "sbyte",
    "sealed",
    "short",
    "sizeof",
    "stackalloc",
    "static",
    "string",
    "struct",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "uint",
    "ulong",
    "unchecked",
    "unsafe",
    "ushort",
    "using",
    "virtual",
    "void",
    "volatile",
    "while",
];

pub(crate) fn namespace_segment(source: &str) -> String {
    escape_identifier(&pascal_case(source))
}

pub(crate) fn method_name(source: &str) -> String {
    let (base, suffix) = source
        .split_once('$')
        .map_or((source, None), |(base, suffix)| (base, Some(suffix)));
    let mut name = pascal_case(base);
    if let Some(suffix) = suffix {
        name.push_str(&pascal_case(suffix));
    }
    escape_identifier(&name)
}

pub(crate) fn parameter_name(source: &str) -> String {
    escape_identifier(&camel_case(source))
}

pub(crate) fn allocate(preferred: &str, identity: &str, occupied: &mut BTreeSet<String>) -> String {
    if occupied.insert(preferred.to_string()) {
        return preferred.to_string();
    }

    allocate_hashed(preferred, identity, stable_hash(identity), occupied)
}

pub(crate) fn allocate_scope(
    requests: impl IntoIterator<Item = (String, String)>,
    occupied: &mut BTreeSet<String>,
) -> BTreeMap<String, String> {
    allocate_scope_with_hasher(requests, occupied, stable_hash)
}

pub(crate) fn allocate_type_parameters<'a>(
    owner_identity: &str,
    parameters: impl IntoIterator<Item = &'a str>,
    occupied: &mut BTreeSet<String>,
) -> BTreeMap<String, String> {
    let parameters = parameters
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let allocated = allocate_scope(
        parameters.iter().map(|parameter| {
            (
                namespace_segment(parameter),
                format!("{owner_identity}:type-parameter:{parameter}"),
            )
        }),
        occupied,
    );
    parameters
        .into_iter()
        .map(|parameter| {
            let identity = format!("{owner_identity}:type-parameter:{parameter}");
            (parameter, allocated[&identity].clone())
        })
        .collect()
}

fn allocate_scope_with_hasher(
    requests: impl IntoIterator<Item = (String, String)>,
    occupied: &mut BTreeSet<String>,
    hash_identity: impl Fn(&str) -> u64,
) -> BTreeMap<String, String> {
    let mut by_preferred = BTreeMap::<String, Vec<String>>::new();
    let mut identities = BTreeSet::new();
    for (preferred, identity) in requests {
        assert!(
            identities.insert(identity.clone()),
            "duplicate typed C# name request for {identity}"
        );
        by_preferred.entry(preferred).or_default().push(identity);
    }

    let mut allocated = BTreeMap::new();
    let mut collisions = Vec::new();
    for (preferred, mut group) in by_preferred {
        group.sort();
        if group.len() == 1 && occupied.insert(preferred.clone()) {
            allocated.insert(group.pop().unwrap(), preferred);
        } else {
            collisions.extend(
                group
                    .into_iter()
                    .map(|identity| (preferred.clone(), identity)),
            );
        }
    }

    collisions.sort_by(|left, right| left.1.cmp(&right.1));
    let mut collision_hashes = BTreeMap::new();
    for (preferred, identity) in collisions {
        let hash = hash_identity(&identity);
        if let Some(previous) = collision_hashes.insert((preferred.clone(), hash), identity.clone())
        {
            panic!(
                "the full typed-identity hash collided in one C# lexical scope between {previous} and {identity}"
            );
        }
        let candidate = allocate_hashed(&preferred, &identity, hash, occupied);
        allocated.insert(identity, candidate);
    }
    allocated
}

fn allocate_hashed(
    preferred: &str,
    identity: &str,
    hash: u64,
    occupied: &mut BTreeSet<String>,
) -> String {
    for width in 8..=16 {
        let suffix = format!("_{hash:016x}");
        let candidate = format!("{}{}", preferred, &suffix[..=width]);
        if occupied.insert(candidate.clone()) {
            return candidate;
        }
    }

    panic!("the full typed-identity hash collided in one C# lexical scope for {identity}")
}

fn pascal_case(source: &str) -> String {
    let mut out = String::new();
    let mut capitalize = true;
    for ch in source.chars() {
        if ch.is_ascii_alphanumeric() {
            if out.is_empty() && ch.is_ascii_digit() {
                out.push('_');
            }
            if capitalize {
                out.extend(ch.to_uppercase());
                capitalize = false;
            } else {
                out.push(ch);
            }
        } else {
            capitalize = true;
        }
    }
    if out.is_empty() { "_".to_string() } else { out }
}

fn camel_case(source: &str) -> String {
    let pascal = pascal_case(source);
    let mut chars = pascal.chars();
    let Some(first) = chars.next() else {
        return "_".to_string();
    };
    format!("{}{}", first.to_ascii_lowercase(), chars.as_str())
}

fn escape_identifier(source: &str) -> String {
    if KEYWORDS.contains(&source) {
        format!("@{source}")
    } else {
        source.to_string()
    }
}

fn stable_hash(value: &str) -> u64 {
    value
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_idiomatic_names_and_preserves_keywords() {
        assert_eq!(method_name("classify_sentiment"), "ClassifySentiment");
        assert_eq!(
            method_name("classify$build_request"),
            "ClassifyBuildRequest"
        );
        assert_eq!(parameter_name("class"), "@class");
        assert_eq!(parameter_name("record"), "record");
        assert_eq!(parameter_name("field"), "field");
        assert_eq!(namespace_segment("some_namespace"), "SomeNamespace");
    }

    #[test]
    fn collision_suffix_depends_on_identity_not_discovery_order() {
        let mut occupied = BTreeSet::new();
        let requests = [
            ("FooBar".to_string(), "user.foo_bar".to_string()),
            ("FooBar".to_string(), "user.fooBar".to_string()),
        ];
        let forward = allocate_scope(requests.clone(), &mut occupied);
        let mut reverse_occupied = BTreeSet::new();
        let reverse = allocate_scope(requests.into_iter().rev(), &mut reverse_occupied);

        assert_eq!(forward, reverse);
        assert!(forward.values().all(|name| name.starts_with("FooBar_")));
        assert_eq!(forward.values().collect::<BTreeSet<_>>().len(), 2);
    }

    #[test]
    #[should_panic(expected = "full typed-identity hash collided")]
    fn full_hash_collision_fails_instead_of_using_discovery_order() {
        let mut occupied = BTreeSet::new();
        let requests = [
            ("Foo".to_string(), "first".to_string()),
            ("Foo".to_string(), "second".to_string()),
        ];
        let _ = allocate_scope_with_hasher(requests, &mut occupied, |_| 0x1234);
    }
}
