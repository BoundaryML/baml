function test_type_annotation(function_name: string) -> string {
    let t: reflect.Type = ai.internal.get_return_type(function_name);
    return "ok";
}
