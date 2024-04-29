use internal_baml_core::ir::{FunctionWalker, TestCaseWalker};

use crate::BamlRuntime;

pub struct IrFeatures {
    v1_functions: Vec<String>,
    v2_functions: bool,
    class_getters: Vec<(String, Vec<String>)>,
}

impl IrFeatures {
    pub fn err_if_legacy(&self) -> anyhow::Result<()> {
        // if !self.v1_functions.is_empty() {
        //     return Err(anyhow::anyhow!(
        //         "Legacy functions are not supported.\n{} legacy functions found.\n  {}\nPlease migrate to the new function format. See https://docs.boundaryml.com", self.v1_functions.len(), self.v1_functions.join(", ")
        //     ));
        // }

        if !self.class_getters.is_empty() {
            return Err(anyhow::anyhow!(
                "Legacy @get is not supported.\n{}\nPlease remove them from your code. See https://docs.boundaryml.com",
                self.class_getters
                    .iter()
                    .map(|(class, fields)| {
                        format!(
                            "  {}: {}",
                            class,
                            fields.join(", ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        Ok(())
    }
}

pub trait WithInternal {
    fn features(&self) -> IrFeatures;

    fn walk_functions(&self) -> impl ExactSizeIterator<Item = FunctionWalker>;

    fn walk_tests(&self) -> impl Iterator<Item = TestCaseWalker>;
}

impl WithInternal for BamlRuntime {
    fn features(&self) -> IrFeatures {
        IrFeatures {
            v1_functions: self
                .ir
                .walk_functions()
                .filter(|f| f.is_v1())
                .map(|f| f.name().to_string())
                .collect(),
            v2_functions: self.ir.walk_functions().any(|f| f.is_v2()),
            class_getters: self
                .ir
                .walk_classes()
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
        }
    }

    fn walk_functions(&self) -> impl ExactSizeIterator<Item = FunctionWalker> {
        self.ir.walk_functions()
    }

    fn walk_tests(&self) -> impl Iterator<Item = TestCaseWalker> {
        self.ir.walk_tests()
    }
}
