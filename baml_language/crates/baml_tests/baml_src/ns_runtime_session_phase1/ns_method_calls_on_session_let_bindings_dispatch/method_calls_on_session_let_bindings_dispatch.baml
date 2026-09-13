function main() -> string {
    let s = reflect.Session.new();
    s.eval(
        `
            class P {
              name string
              function greet(self) -> string throws never {
                "hi " + self.name
              }
            }
        `,
    );
    s.eval(`let n = 5`);
    s.eval(`let text = "hi"`);
    s.eval(`let v = ["a", "b"]`);
    s.eval(`let m = { "k": 1, "j": 2 }`);
    s.eval(`let p = P { name: "ada" }`);
    s.eval(`let cls = reflect.class.new("R", { "a": reflect.Type.of<string>() })`);
    s.eval<string>(
        `` `\${n.abs()}|\${text.to_upper_case()}|\${v.length()}|\${m.length()}|\${v.join("-")}|\${m.keys().join("-")}|\${p.greet()}|\${cls.fields()[0].name}` ``,
    )
}

test "method_calls_on_session_let_bindings_dispatch" {
    assert.equal(main(), "5|HI|2|2|a-b|k-j|hi ada|a")
}
