// instanceof operator test

type Tool = RequestInfo | UpdateContext

class RequestInfo {
  type "request_info"
  message string
}

class UpdateContext {
  type "update_context"
  data string
}

function TestInstanceOf() -> string {
  let tool = RequestInfo {
    type: "request_info",
    message: "test"
  };

  if (tool instanceof RequestInfo) {
    tool.message
  } else {
    "not RequestInfo"
  }
}

//> TestInstanceOf()
//
// "test"
