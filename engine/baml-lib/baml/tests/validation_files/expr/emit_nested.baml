function WorkflowWatch() -> int {
  let x: int = 10 @watch;
  let y: bool = true @watch;

  y = false;
  x = WorkflowWatchChild();
  x
}

function WorkflowWatchChild() -> int {
  let x: string = "Hello" @watch;
  100
}
