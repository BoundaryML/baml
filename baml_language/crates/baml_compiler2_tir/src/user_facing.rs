use crate::ty::SYNTHETIC_EFFECT_PARAM_PREFIX;

/// Rewrites synthetic effect-param names (`__effect_param_N` → `callback`) in an
/// already-rendered type string. Called only by the LSP display path. The
/// prefix+digit-run predicate mirrors [`crate::ty::is_synthetic_effect_param`].
pub fn humanize_type_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;

    while let Some(ch) = rest.chars().next() {
        if let Some(after_prefix) = rest.strip_prefix(SYNTHETIC_EFFECT_PARAM_PREFIX) {
            let digit_count = after_prefix.bytes().take_while(u8::is_ascii_digit).count();
            if digit_count > 0 {
                out.push_str("callback");
                rest = &after_prefix[digit_count..];
                continue;
            }
        }
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }

    out
}

#[cfg(test)]
mod tests {
    use super::humanize_type_string;

    #[test]
    fn humanize_effect_param_names_in_function_types() {
        assert_eq!(
            humanize_type_string(
                "(cb: (x: int) -> string throws __effect_param_0) -> string throws __effect_param_0",
            ),
            "(cb: (x: int) -> string throws callback) -> string throws callback"
        );
    }

    #[test]
    fn leaves_non_synthetic_names_unchanged() {
        assert_eq!(
            humanize_type_string("map<string, user.Handler>"),
            "map<string, user.Handler>"
        );
    }
}
