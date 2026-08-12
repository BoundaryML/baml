// Test io operations typecheck correctly

function AskName() -> string {
    baml.io.input("What is your name? ")
}

function AskNoPrompt() -> string {
    baml.io.input(null)
}
