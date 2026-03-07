class PromptAst {
  @@internal.opaque
}

enum ClientType {
  Primitive
  Fallback
  RoundRobin
}

class RetryPolicy {
  max_retries int
  initial_delay_ms int
  multiplier float
  max_delay_ms int
}

class Client {
  name string
  client_type ClientType
  sub_clients Client[]
  retry RetryPolicy?
}

class PrimitiveClient {
  name string
  provider string
  default_role string
  allowed_roles string[]
  options map<string, unknown>

  function render_prompt(
    self,
    template: string,
    args: map<string, unknown>,
  ) -> PromptAst throws baml.errors.RenderPrompt {
    $rust_io_function
  }

  function specialize_prompt(
    self,
    prompt: PromptAst,
  ) -> PromptAst throws baml.errors.RenderPrompt | baml.errors.LlmClient {
    $rust_io_function
  }

  function build_request(
    self,
    prompt: PromptAst,
  ) -> baml.http.Request throws baml.errors.LlmClient {
    $rust_io_function
  }

  function parse(
    self,
    http_response_body: string,
    type_def: type,
  ) -> unknown throws baml.errors.LlmClient {
    $rust_io_function
  }
}

@@internal.uses(engine_ctx)
function get_jinja_template(function_name: string) -> string throws baml.errors.InvalidArgument {
  $rust_io_function
}

function build_primitive_client(
  name: string,
  provider: string,
  default_role: string,
  allowed_roles: string[],
  options: map<string, unknown>,
) -> PrimitiveClient throws baml.errors.InvalidArgument {
  $rust_io_function
}

@@internal.uses(engine_ctx)
function get_client(function_name: string) -> Client throws baml.errors.InvalidArgument {
  $rust_io_function
}

@@internal.uses(engine_ctx)
function resolve_client(
  client_name: string,
) -> () -> PrimitiveClient throws baml.errors.InvalidArgument {
  $rust_io_function
}

@@internal.uses(engine_ctx)
function round_robin_next(client_name: string) -> int throws baml.errors.InvalidArgument {
  $rust_io_function
}

@@internal.uses(engine_ctx)
function round_robin_peek(client_name: string) -> int throws baml.errors.InvalidArgument {
  $rust_io_function
}

@@internal.uses(engine_ctx)
function get_return_type(function_name: string) -> type throws baml.errors.InvalidArgument {
  $rust_io_function
}
