use internal_baml_core::ir::{FunctionWalker, TestCaseWalker};

pub struct IrFeatures {
    v1_functions: Vec<String>,
    v2_functions: bool,
    class_getters: Vec<(String, Vec<String>)>,
}

impl IrFeatures {
    pub fn from(
        v1_functions: Vec<String>,
        v2_functions: bool,
        class_getters: Vec<(String, Vec<String>)>,
    ) -> Self {
        Self {
            v1_functions,
            v2_functions,
            class_getters,
        }
    }

    pub fn err_if_legacy(&self) -> anyhow::Result<()> {
        // TODO: @hellovai consider if we want to keep this check
        // Option 1 (current): instead we give a runtime error if we encounter a legacy function.
        // Option 2 (if uncommented): we require that all legacy functions are removed before the code is used by the runtime.

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
