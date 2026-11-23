use baml_base::LangPackage;

use crate::Name;

/// The segment every dependency outside the SDK root lives under.
const VENDOR_SEGMENT: &str = "vendor";

/// Whether a stdlib package lives at the SDK root rather than under
/// [`VENDOR_SEGMENT`]. This is SDK layout policy: the core language packages
/// live at the root, every other dependency under `vendor`.
const fn at_sdk_root(package: LangPackage) -> bool {
    match package {
        LangPackage::Baml | LangPackage::Reflect | LangPackage::Ai => true,
        LangPackage::Log
        | LangPackage::Trace
        | LangPackage::Testing
        | LangPackage::Assert
        | LangPackage::Openai
        | LangPackage::Anthropic
        | LangPackage::Typesafeai
        | LangPackage::Google
        | LangPackage::Aws
        | LangPackage::Vercel
        | LangPackage::ClaudeCode => false,
    }
}

/// Package and namespace segments before target-language identifier escaping.
/// Core language packages live at the SDK root; other dependencies live under vendor.
// BUG: not injective. A local namespace spelled like a dependency root
// (`baml`, `ai`, `reflect`, `vendor`) gets the same segments as the
// dependency, so the SDKs merge the two into one module.
pub fn namespace_segments(name: &Name) -> Vec<String> {
    let mut segments = Vec::new();
    if !name.is_local() {
        let package = name.package().as_str();
        if !LangPackage::from_manifest_name(package).is_some_and(at_sdk_root) {
            segments.push(VENDOR_SEGMENT.to_owned());
        }
        segments.push(package.to_owned());
    }
    segments.extend(name.namespace().iter().map(|part| part.as_str().to_owned()));
    segments
}

/// The first segment [`namespace_segments`] gives every name outside the
/// local package: the root modules dependencies live in.
pub fn dependency_root_segments() -> impl Iterator<Item = &'static str> {
    LangPackage::ALL
        .into_iter()
        .filter(|package| at_sdk_root(*package))
        .map(LangPackage::manifest_name)
        .chain([VENDOR_SEGMENT])
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    #[test]
    fn builtin_and_external_packages_have_distinct_roots() {
        for (package, expected) in [
            ("user", vec!["nested"]),
            ("baml", vec!["baml", "nested"]),
            ("ai", vec!["ai", "nested"]),
            ("reflect", vec!["reflect", "nested"]),
            ("trace", vec!["vendor", "trace", "nested"]),
            ("aws", vec!["vendor", "aws", "nested"]),
            ("vendor_pkg", vec!["vendor", "vendor_pkg", "nested"]),
        ] {
            let name = Name::new(package.into(), vec!["nested".into()], "Thing".into());
            assert_eq!(namespace_segments(&name), expected);
        }
    }

    #[test]
    fn dependency_roots_are_the_first_segments_dependencies_get() {
        let first_segments: BTreeSet<String> = LangPackage::ALL
            .into_iter()
            .map(LangPackage::manifest_name)
            .chain(["vendor_pkg"])
            .map(|package| {
                let name = Name::new(package.into(), Vec::new(), "Thing".into());
                namespace_segments(&name).swap_remove(0)
            })
            .collect();
        assert_eq!(
            first_segments,
            dependency_root_segments()
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
        );
    }
}
