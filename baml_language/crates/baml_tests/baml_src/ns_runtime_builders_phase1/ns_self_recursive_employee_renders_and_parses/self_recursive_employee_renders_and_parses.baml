client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

function Extract<T>() -> T {
    client: TestClient
    prompt: `${ctx.output_format()}`
}

function main() -> string {
    let employee = reflect.class.builder("Employee");
    let pending = employee.type();
    employee.field(
        "name",
        reflect.Type.of<string>().meta(
            alias = "full_name",
            description = "the employee's display name",
        ),
    );
    employee.field("manager", pending.optional());
    let employee_t = employee.build();
    type Employee = unreflect(employee_t.as_type())
    let prompt = Extract@render_prompt<Employee>().text();
    let value = Extract@parse<Employee>(`{"full_name":"Ada","manager":{"full_name":"Grace","manager":null}`);
    let manager = reflect.class.get_field<unknown>(value, "manager");
    let manager_name = reflect.class.get_field<string>(manager, "name");
    return prompt
        + "\n<RESULT>"
        + manager_name
        + "|"
        + (pending.resolved() == employee_t.as_type()).to_string()
        + "|"
        + baml.json.to_string(value);
}

test "self_recursive_employee_renders_and_parses" {
    let result = main();
    assert.contains(result, "Employee");
    assert.is_true(
        result.ends_with(
            "<RESULT>Grace|true|{\"name\":\"Ada\",\"manager\":{\"name\":\"Grace\",\"manager\":null}}",
        ),
    )
}
