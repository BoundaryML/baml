// Test io operations typecheck correctly

function AskName() -> string {
    io.input("What is your name? ")
}

function AskNoPrompt() -> string {
    io.input(null)
}
