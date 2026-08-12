class JsonData {
  field1 int
  field2 int
}

function ProcessData(data: JsonData) -> string {
  let result = data.field1 + data.field2;
  if (result > 100) {
    return "high";
  }
  return "low";
}
