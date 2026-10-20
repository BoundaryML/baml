// A binding named like a language package is ordinary, and the body of the
// `spawn` beside it is checked like any other body: the undefined name
// inside is reported.

function task_body_still_checked() -> int {
    let baml = 5;
    let task = spawn { undefined_in_body + baml };
    0
}
