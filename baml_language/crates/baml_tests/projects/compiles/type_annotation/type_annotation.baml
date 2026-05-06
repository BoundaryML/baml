function test_type_annotation(function_name: string) -> string {
    let t: type = baml.llm.get_return_type(function_name);
    return "ok";
}
