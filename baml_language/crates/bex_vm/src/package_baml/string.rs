use bex_vm_types::types::Value;

use super::*;
use crate::BexVm;

impl BamlClassString for PackageBamlImpl {
    #[allow(clippy::cast_possible_wrap)]
    fn length(string: &str) -> i64 {
        string.chars().count() as i64
    }

    fn to_lower_case(string: &str) -> String {
        string.to_lowercase()
    }

    fn to_upper_case(string: &str) -> String {
        string.to_uppercase()
    }

    fn trim(string: &str) -> String {
        string.trim().to_string()
    }

    fn includes(string: &str, search: &str) -> bool {
        string.contains(search)
    }

    fn starts_with(string: &str, prefix: &str) -> bool {
        string.starts_with(prefix)
    }

    fn ends_with(string: &str, suffix: &str) -> bool {
        string.ends_with(suffix)
    }

    fn split(vm: &mut BexVm, string: &str, delimiter: &str) -> Vec<Value> {
        string
            .split(delimiter)
            .map(|s| vm.alloc_string(s.to_string()))
            .collect()
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn substring(string: &str, start: i64, end: i64) -> String {
        let len = string.len();
        let start = (start as usize).min(len);
        let end = (end as usize).min(len).max(start);
        string[start..end].to_string()
    }

    fn replace(string: &str, search: &str, replacement: &str) -> String {
        string.replacen(search, replacement, 1)
    }

    #[allow(clippy::cast_possible_wrap)]
    fn index_of(string: &str, search: &str) -> i64 {
        string.find(search).map(|i| i as i64).unwrap_or(-1)
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn char_at(string: &str, index: i64) -> String {
        string
            .chars()
            .nth(index as usize)
            .map(|c| c.to_string())
            .unwrap_or_default()
    }

    fn matches(string: &str, pattern: &str) -> bool {
        string.contains(pattern)
    }

    fn replace_all(string: &str, search: &str, replacement: &str) -> String {
        string.replace(search, replacement)
    }
}
