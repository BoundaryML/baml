function f() -> int {
    let response: root.fixtures.patterns_class_destructure.llm.openai.Response = root.fixtures.patterns_class_destructure.llm.openai.Response {
        text: "ok",
        score: 43,
    };
    let short = Response { text: "ok" };
    let mapped: T = some.long.path.Map<int, int, never, never> { aaaaaaaaaaaaaaaaaaaaaaaaaaaaa: 1, bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb: 2 };
    1
}
