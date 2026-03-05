function ConditionalLogic(age: int) -> string {
  if (age < 18) {
    return "minor";
  } else if (age < 65) {
    return "adult";
  } else {
    return "senior";
  }
}
