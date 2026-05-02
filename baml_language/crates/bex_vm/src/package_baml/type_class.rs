use bex_vm_types::types::{Object, Value};

use super::{BamlClassTypeValue, PackageBamlImpl};
use crate::BexVm;

impl BamlClassTypeValue for PackageBamlImpl {
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
