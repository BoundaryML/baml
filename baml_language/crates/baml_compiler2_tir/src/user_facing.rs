use crate::ty::Ty;

const SYNTHETIC_EFFECT_PARAM_PREFIX: &str = "__effect_param_";

pub fn humanize_type_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let remainder = &raw[index..];
        if let Some(rest) = remainder.strip_prefix(SYNTHETIC_EFFECT_PARAM_PREFIX) {
            let digit_count = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digit_count > 0 {
                out.push_str("callback");
                index += SYNTHETIC_EFFECT_PARAM_PREFIX.len() + digit_count;
                continue;
            }
        }

        let ch = remainder
            .chars()
            .next()
            .expect("remainder is non-empty while scanning type string");
        out.push(ch);
        index += ch.len_utf8();
    }

    out
}

pub fn humanize_ty(ty: &Ty) -> String {
    humanize_type_string(&ty.to_string())
}

pub fn humanize_type_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    names.into_iter().map(humanize_type_string).collect()
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
