/// The three question and answer kinds supported by System One.
type QuestionKind = "noul" | "choice" | "score";

/// One native System One question. Use `noul`, `choice`, or `score` to create it.
class Question {
    type: QuestionKind,
    instructions: baml.json.json,
    criteria: baml.json.json?,
}

/// The native answer, including the distribution and confidence when supplied.
class Answer {
    type: QuestionKind,
    noul: float?,
    choice: string?,
    score: float?,
    probabilities: map<string, float>?,
    legend: map<string, string>?,
    confidence: float?,
}

class Usage {
    input_tokens: int,
    output_tokens: int,
}

class Response {
    model: string,
    answers: map<string, Answer>,
    usage: Usage,
}

class ModelCard {
    name: string,
    description: string,
    release_date: string,
}

class ModelsResponse {
    models: ModelCard[],
}

/// A yes/no question. `criteria` may describe the true and false cases.
function noul(instructions: baml.json.json, criteria: map<string, string>? = null) -> Question {
    Question { type: "noul", instructions: instructions, criteria: baml.json.from(criteria) }
}

/// Pick one of the named options. An option description may be null.
function choice(instructions: baml.json.json, criteria: map<string, string?>) -> Question {
    Question { type: "choice", instructions: instructions, criteria: baml.json.from(criteria) }
}

/// Rate across two or more ordered levels.
function score(instructions: baml.json.json, criteria: string[]) -> Question {
    Question { type: "score", instructions: instructions, criteria: baml.json.from(criteria) }
}
