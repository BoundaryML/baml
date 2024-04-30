use internal_baml_core::ir::{FunctionWalker, TestCaseWalker};

use crate::{
    internal::ir_features::{IrFeatures, WithInternal},
    InternalRuntimeInterface,
};

use super::InternalBamlRuntime;

impl WithInternal for InternalBamlRuntime {
    fn features(&self) -> IrFeatures {
        let ir = self.ir();

        IrFeatures::from(
            ir.walk_functions()
                .filter(|f| f.is_v1())
                .map(|f| f.name().to_string())
                .collect(),
            ir.walk_functions().any(|f| f.is_v2()),
            ir.walk_classes()
                .filter(|c| !c.elem().dynamic_fields.is_empty())
                .map(|c| {
                    (
                        c.name().to_string(),
                        c.elem()
                            .dynamic_fields
                            .iter()
                            .map(|f| f.elem.name.to_string())
                            .collect(),
                    )
                })
                .collect(),
        )
    }

    fn walk_functions(&self) -> impl ExactSizeIterator<Item = FunctionWalker> {
        self.ir().walk_functions()
    }

    fn walk_tests(&self) -> impl Iterator<Item = TestCaseWalker> {
        self.ir().walk_tests()
    }
}
