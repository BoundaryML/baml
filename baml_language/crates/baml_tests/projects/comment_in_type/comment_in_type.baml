// Test case for comments in type positions

client<llm> TestClient {
  provider baml-openai-chat
}

// This function has comments around the return type
function FunctionWithComments(
  a: string,
  b: int // comment after param type
)
// Comment before arrow
->
// Comment after arrow
string
// Comment after return type
{
  client TestClient
  prompt #"test"#
}

// Another test - comment immediately after arrow
function CommentAfterArrow() -> // inline comment
string {
  client TestClient
  prompt #"test"#
}
