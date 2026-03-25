use super::{BamlNamespaceMath, PackageBamlImpl};

impl BamlNamespaceMath for PackageBamlImpl {
    #[allow(clippy::cast_possible_truncation)]
    fn trunc(value: f64) -> i64 {
        value as i64
    }
}
