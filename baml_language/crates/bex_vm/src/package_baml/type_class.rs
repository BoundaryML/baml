use bex_vm_types::types::{Object, Value};

use super::{BamlClassTypeValue, PackageBamlImpl};
use crate::BexVm;

impl BamlClassTypeValue for PackageBamlImpl {
    /// Returns the `Ty`'s display name.  Includes namespaces and (for
    /// non-`user` packages) the package prefix, so two distinct types never
    /// collide on this string — package names are unique within a workspace,
    /// so eliding the implicit `user.` prefix is unambiguous.
    ///
    /// This identity guarantee makes the result usable as a stable key in
    /// `map<string, V>` until generic-K interfaces enable a real
    /// `map<type, V>`.
    fn to_string(vm: &BexVm, self_value: &Value) -> String {
        let Value::Object(ptr) = self_value else {
            return "<type: ?>".to_string();
        };
        match vm.get_object(*ptr) {
            Object::Type(ty) => ty.to_string(),
            _ => "<type: ?>".to_string(),
        }
    }
}
