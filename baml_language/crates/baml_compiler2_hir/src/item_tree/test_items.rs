use baml_base::Name;
pub use baml_compiler2_ast::ast::TestArgValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Test {
    pub name: Name,
    /// The function(s) this test exercises.
    pub function_refs: Vec<Name>,
    /// Test arguments as key-value pairs.
    pub args: Vec<(Name, TestArgValue)>,
}
