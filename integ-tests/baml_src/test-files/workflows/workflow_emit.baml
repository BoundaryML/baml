function WorkflowEmit() -> int {
  let x: int = 10 @emit;
  let y: bool = true @emit;

  y = false;
  x = WorkflowEmitChild();
  x = 101;
  x
}

function WorkflowEmitChild() -> int {
  let x: string = "Hello" @emit;
  100
}

test TestWorkflowEmit() {
  functions [WorkflowEmit]
  args {}
}
