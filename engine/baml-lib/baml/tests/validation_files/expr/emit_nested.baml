function WorkflowEmit() -> int {
  let x: int = 10 @emit;
  let y: bool = true @emit;

  y = false;
  x = WorkflowEmitChild();
  x
}

function WorkflowEmitChild() -> int {
  let x: string = "Hello" @emit;
  100
}
