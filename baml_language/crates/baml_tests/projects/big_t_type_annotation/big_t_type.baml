function test_big_t_type(function_name: string) -> string {
    let t: big_t_type = baml.llm.get_return_type(function_name);
    return "ok";
}
