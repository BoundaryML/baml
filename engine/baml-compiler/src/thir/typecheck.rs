use crate::hir::{Expression, Hir, Type};
use crate::thir::{ExprMetadata, THir};
use baml_types::BamlMap;
use internal_baml_diagnostics::Diagnostics;

pub fn typecheck(hir: &Hir, diagnostics: &mut Diagnostics) -> THir<ExprMetadata> {
    let llm_functions = hir.llm_functions.clone();
    let classes = hir
        .classes
        .clone()
        .into_iter()
        .map(|c| (c.name.clone(), c))
        .collect();

    let enums = hir
        .enums
        .clone()
        .into_iter()
        .map(|e| (e.name.clone(), e))
        .collect();

    THir {
        llm_functions,
        classes,
        enums,
        expr_functions: vec![],
        global_assignments: BamlMap::new(),
    }
}

#[derive(Clone, Debug)]
pub struct TypeContext {
    pub inner: BamlMap<String, Type>,
}

impl TypeContext {
    pub fn get_type(&self, name: &str) -> Option<&Type> {
        self.inner.get(name)
    }

    pub fn local_assignment(self, name: &str, r#type: Type) -> Self {
        let mut env_copy = self.clone();
        env_copy.inner.insert(name.to_string(), r#type);
        env_copy
    }

    pub fn infer_type(context: &mut Self, expr: &Expression) -> Option<Type> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::hir::Hir;
    use baml_types::BamlMap;
    use internal_baml_diagnostics::Diagnostics;

    /// Test helper to generate HIR from BAML source
    fn hir_from_source(source: &str) -> (Hir, Diagnostics) {
        let diagnostics = Diagnostics::new(PathBuf::from("test.baml"));
        let db = crate::test::ast(source).unwrap_or_else(|e| panic!("{}", e));
        (Hir::from_ast(&db.ast), diagnostics)
    }

    #[test]
    fn infer_primitive_types() {
        let (hir, mut diagnostics) = hir_from_source(
            r##"
          let a = 1;
          let b = 2.0;
          let c = "hello";
        "##,
        );
        let thir = typecheck(&hir, &mut diagnostics);
        let a_value = thir.global_assignments.get("a").expect("a is present");
        a_value
            .meta()
            .1
            .as_ref()
            .expect("a should be inferred")
            .assert_eq_up_to_span(&Type::int())
    }
}
