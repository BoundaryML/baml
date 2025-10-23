function WorkflowWatch() -> int {
  watch let x: int = 10;
  x = x + 1;
  watch let y: bool = true;
  watch let once: string = "Hello";
  once = "World"; // Change once so it triggers notification
  watch let twice: string[] = ["Takedown", ""];
  twice = ["Takedown", "Takedown"]; // Change twice so it triggers notification
  watch let story = LLMEcho(#"
    Hello world this is a test of a longish string.
    I want it to be long enough to generate a few chunks.
    "#);

  y = false;
  x = WorkflowWatchChild();
  x = 101;
  x
}

function WorkflowWatchChild() -> int {
  watch let x: string = "InitialValue";
  x = "ChangedValue"; // Change x so it triggers notification
  100
}

function AnotherTakedown(xs: string[]) -> int {
  xs[1] = "Takedown";
  xs = ["Takedown", "Takedown"];
  0
}

// Filter function that uses an LLM to check if a word equals "banana"
// This tests that filter functions can call LLMs
function IsTargetWord(word: string) -> bool {
  let result = CheckWordEquality(word, "banana");
  result
}

function IsTargetWord2(word: string) -> bool {
  word == "banana"
}

// LLM function that checks if two words are equal
// We use an LLM for this trivial task just to test LLM calling in filter functions
function CheckWordEquality(word: string, target: string) -> bool {
  client GPT4Turbo
  prompt #"
    Does the word "{{ word }}" equal "{{ target }}"?

    Respond with only true or false.
  "#
}

// Test with LLM-based filter function
function WorkflowWatchWithFilter() -> int {
  let words: string[] = ["apple", "banana", "cherry", "banana", "date",
                          "elderberry", "banana", "fig", "grape", "honeydew"];

  // This variable will only notify watchers when the word is "banana" (as determined by LLM)
  watch let this_word: string = "";
  this_word.$watch.options( baml.WatchOptions{ when: IsTargetWord })

  let i: int = 0;
  for (let word in words) {
    this_word = word;
    i += 1;
  }

  // Should have notified 3 times (for the 3 "banana" occurrences at indices 1, 3, 6)
  i
}

test TestWorkflowWatch() {
  functions [WorkflowWatch]
  args {}
}

test TestWorkflowWatchWithFilter() {
  functions [WorkflowWatchWithFilter]
  args {}
}
