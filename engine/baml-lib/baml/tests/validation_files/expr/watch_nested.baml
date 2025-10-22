function WorkflowWatch() -> int {
  watch let x: int = 10;
  watch let y: bool = true;

  y = false;
  x = WorkflowWatchChild();
  x
}

function WorkflowWatchChild() -> int {
  watch let x: string = "Hello";
  100
}
