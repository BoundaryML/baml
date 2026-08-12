function test_type_annotation(function_name: string) -> string {
    let t: type = baml.prompt.get_return_type(function_name);
    return "ok";
}
