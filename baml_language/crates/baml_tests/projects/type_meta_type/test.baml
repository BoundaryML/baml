// Test that get_return_type produces a Type value and parse accepts it.
// This verifies the full compilation pipeline handles the Type meta-type:
// - TIR infers Ty::Type for the return_type variable
// - MIR lowers it correctly
// - Bytecode emits get_return_type as a sys_op and passes the result to parse

function ExtractName() -> string {
  client "openai/gpt-4o"
  prompt #"Extract the name"#
}

// Mirrors call_llm_function but makes get_return_type explicit
function TestTypeValue(response: string) -> unknown {
  let resolve_fn = baml.llm.get_client_function("ExtractName");
  let pc = resolve_fn();
  let rt = baml.llm.get_return_type("ExtractName");
  return pc.parse(response, rt);
}
