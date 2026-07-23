// Scenario 49 — provider-owned session state across HTTP requests.
//
// The HTTP boundary carries only SessionToken. The live Session resource owns
// its configured provider, remote identity, and lifecycle and is never encoded.
// Request<T>, Provider, and the prompt-render closure remain process-local.

class s49_ContinueInput {
  token: root.ai.SessionToken?,
  message: string,
}

class s49_ContinueOutput {
  token: root.ai.SessionToken,
  reply: string,
}

function s49_prompt_template(
  text: string,
) -> (baml.llm.Context) -> baml.llm.PromptAst throws never {
  (ctx: baml.llm.Context) -> baml.llm.PromptAst {
    baml.llm.assemble_prompt_ast(["", ""], [text])
  }
}

class s49_SessionProvider {
  histories: map<string, string>,
  next_id: int,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s49-session-provider" }
  }

  implements root.ai.Sessions {
    function open_session(self, options: root.ai.SessionOptions) -> root.ai.Session
        throws baml.errors.UnknownError {
      let id = `session-${self.next_id}`;
      self.next_id = self.next_id + 1;
      let _ = self.histories.set(id, "");
      s49_SessionResource { owner: self, id: id }
    }

    function resume_session(self, token: root.ai.SessionToken) -> root.ai.Session
        throws baml.errors.UnknownError {
      if (token.provider != self.display_name() || self.histories.get(token.id) == null) {
        throw baml.errors.UnknownError {
          data: token,
          message: ["session token does not belong to this configured provider"],
        };
      }
      s49_SessionResource { owner: self, id: token.id }
    }
  }
}

class s49_SessionResource {
  owner: s49_SessionProvider,
  id: string,

  implements root.ai.Session {
    function run<T>(self, request: root.ai.Request<T>) -> T throws baml.errors.UnknownError {
      // Session execution context wins unconditionally: rerender on the owning
      // configured provider before reading this task invocation.
      let rebound = request.for_provider(self.owner);
      let prompt_text = baml.llm.prompt_to_text(rebound.prompt);
      let history = self.owner.histories.get(self.id) ?? "";
      let reply = if (
        history.to_lower_case().includes("my name is fred")
          && prompt_text.to_lower_case().includes("what is my name")
      ) {
        "Fred"
      } else {
        "ack"
      };
      let _ = self.owner.histories.set(
        self.id,
        history + "\nuser: " + prompt_text + "\nassistant: " + reply,
      );
      baml.json.from_string<T>(baml.json.to_string<string>(reply)) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["session result parse failed"] },
      }
    }

    function run_with_meta<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.UnknownError {
      root.ai.Response<T> {
        value: self.run<T>(request),
        meta: root.ai.Meta {
          provider: self.owner.display_name(),
          model: "fake-session",
          request_id: self.id,
          finish_reason: "stop",
          usage: null,
          attributes: { "session_id": self.id },
          raw: null,
        },
      }
    }

    function provider(self) -> root.ai.Provider throws never { self.owner }

    function token(self) -> root.ai.SessionToken throws never {
      root.ai.SessionToken { provider: self.owner.display_name(), id: self.id }
    }

    function cleanup(self) -> null throws never { null }
  }
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s49_ContinueInSession(message: string) -> string {
//   client: s49_SessionProvider { histories: {}, next_id: 0 }
//   prompt: `${role("user")} ${message}`
// }

// Hand-written reference expansion of the compiler task surface for
// `s49_ContinueInSession`; the helper name is scaffolding, not a literal
// generated name:
// - `s49_ContinueInSession_request` represents
//   `s49_ContinueInSession.request(...)`.
// The service's `session.run(request)` and resume logic are application-level
// consumers of that request, not compiler-generated task companions.
function s49_ContinueInSession_request(
  message: string,
  provider: root.ai.Provider = s49_SessionProvider { histories: {}, next_id: 0 },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s49_ContinueInSession",
    "ns_ai_scenarios2",
    { "message": message },
    [],
    root.ai.RequestOptions {},
    {},
    s49_prompt_template(message),
  )
}

function s49_resume_session(
  provider: root.ai.Provider,
  token: root.ai.SessionToken,
) -> root.ai.Session throws baml.errors.UnknownError {
  let sessions = match (provider) {
    let capability: root.ai.Sessions => capability,
    _ => throw baml.errors.UnknownError {
      data: provider,
      message: ["configured provider cannot resume sessions"],
    },
  };
  sessions.resume_session(token)
}

class s49_SessionService {
  provider: root.ai.Provider,

  function process(self, input: s49_ContinueInput) -> s49_ContinueOutput
      throws baml.errors.UnknownError {
    let session: root.ai.Session = match (input.token) {
      null => root.ai.open_session(self.provider, root.ai.SessionOptions {}),
      let token: root.ai.SessionToken => s49_resume_session(self.provider, token),
    };
    defer { session.cleanup() }

    let reply = session.run<string>(s49_ContinueInSession_request(input.message));
    s49_ContinueOutput { token: session.token(), reply: reply }
  }

  function handle(self, req: baml.http.Request) -> baml.http.Response {
    let body = baml.json.to_string(
      self.process(baml.json.from_string<s49_ContinueInput>(req.body)),
    ) catch_all (e) {
      _ => "{\"error\":\"session request failed\"}",
    };
    baml.http.Response.new(200, { "content-type": "application/json" }, body.to_utf8())
  }
}

function s49_call_server(addr: string, input: s49_ContinueInput) -> s49_ContinueOutput {
  let response = baml.http.send(baml.http.Request {
    method: "POST",
    url: "http://" + addr + "/session/continue",
    headers: { "content-type": "application/json" },
    body: baml.json.to_string(input),
  });
  baml.json.from_string<s49_ContinueOutput>(response.text())
}

testset "integ-test-49-http-provider-session" {
  test "49: a later HTTP request resumes provider state from SessionToken only" {
    let configured = s49_SessionProvider { histories: {}, next_id: 0 };
    let service = s49_SessionService { provider: configured };
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn { server.serve(service.handle) };

    let first = s49_call_server(server.addr, s49_ContinueInput {
      token: null,
      message: "My name is Fred. Reply with just ack.",
    });
    let second = s49_call_server(server.addr, s49_ContinueInput {
      token: first.token,
      message: "What is my name?",
    });
    task.cancel();

    assert.equal(second.reply, "Fred")
    assert.equal(second.token.id, first.token.id)
  }
}
