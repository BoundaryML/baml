function DestructureQualifiedLet() -> int {
    let response = root.fixtures.patterns_class_destructure_namespaces.llm.openai.Response {
        text: "ok",
        score: 41
    };

    let root.fixtures.patterns_class_destructure_namespaces.llm.openai.Response { score } = response;
    score
}

function DestructureQualifiedMatch() -> int {
    let response = root.fixtures.patterns_class_destructure_namespaces.llm.openai.Response {
        text: "ok",
        score: 42
    };

    match (response) {
        root.fixtures.patterns_class_destructure_namespaces.llm.openai.Response { score: 0 } => 0,
        root.fixtures.patterns_class_destructure_namespaces.llm.openai.Response { score: let matched } => matched
    }
}

function TypePatternQualifiedLet() -> int {
    let response: root.fixtures.patterns_class_destructure_namespaces.llm.openai.Response = root.fixtures.patterns_class_destructure_namespaces.llm.openai.Response {
        text: "ok",
        score: 43
    };

    response.score
}

function TypePatternQualifiedMatch() -> int {
    let response = root.fixtures.patterns_class_destructure_namespaces.llm.openai.Response {
        text: "ok",
        score: 44
    };

    match (response) {
        root.fixtures.patterns_class_destructure_namespaces.llm.openai.Response => response.score
    }
}

function GenericDestructureQualifiedLet() -> int {
    let boxed: root.fixtures.patterns_class_destructure_namespaces.llm.openai.Box<int> = root.fixtures.patterns_class_destructure_namespaces.llm.openai.Box<int> {
        value: 49
    };

    let root.fixtures.patterns_class_destructure_namespaces.llm.openai.Box<int> { value } = boxed;
    value
}

function GenericTypePatternQualifiedMatch() -> int {
    let boxed: root.fixtures.patterns_class_destructure_namespaces.llm.openai.Box<int> | root.fixtures.patterns_class_destructure_namespaces.llm.openai.Box<string> = root.fixtures.patterns_class_destructure_namespaces.llm.openai.Box<int> {
        value: 50
    };

    match (boxed) {
        root.fixtures.patterns_class_destructure_namespaces.llm.openai.Box<string> => 0,
        root.fixtures.patterns_class_destructure_namespaces.llm.openai.Box<int> => 50
    }
}

function DeepGenericDestructureQualifiedLet() -> int {
    let outer: root.fixtures.patterns_class_destructure_namespaces.llm.openai.Outer<int> = root.fixtures.patterns_class_destructure_namespaces.llm.openai.Outer<int> {
        inner: root.fixtures.patterns_class_destructure_namespaces.llm.openai.Box<int> { value: 52 }
    };

    let root.fixtures.patterns_class_destructure_namespaces.llm.openai.Outer<int> {
        inner: root.fixtures.patterns_class_destructure_namespaces.llm.openai.Box<int> { value }
    } = outer;
    value
}
