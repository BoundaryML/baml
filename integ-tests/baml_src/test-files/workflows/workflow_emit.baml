function WorkflowWatch() -> int {
  let x: int = 10 @watch;
  let y: bool = true @watch;
  let once: string = "Hello" @watch;
  let twice: string[] = ["Takedown", ""] @watch;
  let story = LLMEcho(#"
    Hello world this is a test of a longish string.
    I want it to be long enough to generate a few chunks.
    "#) @watch;

  y = false;
  x = WorkflowWatchChild();
  x = 101;
  x
}

function WorkflowWatchChild() -> int {
  let x: string = "Hello" @watch;
  100
}

function AnotherTakedown(xs: string[]) -> int {
  xs[1] = "Takedown";
  xs = ["Takedown", "Takedown"];
  0
}

// Filter function that uses an LLM to check if a word equals "banana"
// This tests that filter functions can call LLMs
function IsTargetWord(prev: string, next: string) -> bool {
  let result = CheckWordEquality(next, "banana");
  result
}

function IsTargetWord2(prev: string, next: string) -> bool {
  next == "banana"
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
  let this_word: string = "" @watch(when=IsTargetWord);

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
