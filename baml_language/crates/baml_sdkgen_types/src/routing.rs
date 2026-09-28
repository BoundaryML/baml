use baml_base::LangPackage;

use crate::Name;

/// Package and namespace segments before target-language identifier escaping.
/// Core language packages live at the SDK root; other dependencies live under vendor.
pub fn namespace_segments(name: &Name) -> Vec<String> {
    let mut segments = Vec::new();
    if !name.is_local() {
        // This is SDK layout policy, not a list of all builtin packages.
        // Bundled provider packages and `trace` retain their vendor paths.
        let at_sdk_root = [LangPackage::Baml, LangPackage::Ai, LangPackage::Reflect]
            .iter()
            .any(|package| package.manifest_name() == name.package().as_str());
        if !at_sdk_root {
            segments.push("vendor".to_owned());
        }
        segments.push(name.package().as_str().to_owned());
    }
    segments.extend(name.namespace().iter().map(|part| part.as_str().to_owned()));
    segments
}

#[cfg(test)]
mod tests {
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
}
