use internal_baml_core::ir::{FunctionWalker, TestCaseWalker};

use crate::BamlRuntime;

pub struct IrFeatures {
    pub v1_functions: bool,
    pub v2_functions: bool,
    pub class_getters: bool,
}

pub trait WithInternal {
    fn features(&self) -> IrFeatures;

    fn walk_functions(&self) -> impl ExactSizeIterator<Item = FunctionWalker>;

    fn walk_tests(&self) -> impl Iterator<Item = TestCaseWalker>;
}

impl WithInternal for BamlRuntime {
    fn features(&self) -> IrFeatures {
        IrFeatures {
            v1_functions: self.ir.walk_functions().any(|f| f.is_v1()),
            v2_functions: self.ir.walk_functions().any(|f| f.is_v2()),
            class_getters: self
                .ir
                .walk_classes()
                .any(|c| c.item.attributes.get("get").is_some()),
        }
    }

    fn walk_functions(&self) -> impl ExactSizeIterator<Item = FunctionWalker> {
        self.ir.walk_functions()
    }

    fn walk_tests(&self) -> impl Iterator<Item = TestCaseWalker> {
        self.ir.walk_tests()
    }
}
