class Response {
    text string
    score int
}

class Box<T> {
    value T
}

class Outer<T> {
    inner Box<T>
}

function DestructureBareInNamespace(response: Response) -> int {
    let Response { score } = response;
    score
}
