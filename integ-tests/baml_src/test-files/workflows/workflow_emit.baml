function WorkflowEmit() -> int {
  let x: int = 10 @emit;
  let y: bool = true @emit;
  let once: string = "Hello" @emit;
  let twice: string[] = ["Takedown", ""] @emit;

  y = false;
  x = WorkflowEmitChild();
  x = 101;
  x
}

function WorkflowEmitChild() -> int {
  let x: string = "Hello" @emit;
  100
}

function AnotherTakedown(xs: string[]) -> int {
  xs[1] = "Takedown";
  xs = ["Takedown", "Takedown"];
  0
}

test TestWorkflowEmit() {
  functions [WorkflowEmit]
  args {}
}
