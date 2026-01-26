function MyFunction1(message: string) -> string {
  //# Start
  watch let greeting = message + " world";

  //# Finish
  greeting
}

function Main() -> string {
  MyFunction1("hello")
}

test default {
  functions [Main]
  args { }
}
