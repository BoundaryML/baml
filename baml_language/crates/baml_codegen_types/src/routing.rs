use crate::Name;

/// Package and namespace segments before target-language identifier escaping.
/// Builtin packages live at the SDK root; external packages live under vendor.
pub fn namespace_segments(name: &Name) -> Vec<String> {
    let mut segments = Vec::new();
    if !name.is_local() {
        match name.package().as_str() {
            "baml" | "ai" | "reflect" => segments.push(name.package().as_str().to_owned()),
            package => {
                segments.push("vendor".to_owned());
                segments.push(package.to_owned());
            }
        }
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
            ("vendor_pkg", vec!["vendor", "vendor_pkg", "nested"]),
        ] {
            let name = Name::new(package.into(), vec!["nested".into()], "Thing".into());
            assert_eq!(namespace_segments(&name), expected);
        }
    }
}
