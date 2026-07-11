// Scenario 48 — app-owned conversation state across HTTP requests.
//
// This is deliberately after the original 47-scenario corpus. It shows the
// smallest honest server shape when the APPLICATION owns history:
//
//   HTTP request -> load serializable transcript -> Request<T> -> run -> save transcript
//
// Only `s48_Transcript` is persisted. `Provider`, `Request<T>`, prompt closures,
// streams, and futures stay process-local.

class s48_Transcript {
  turns: root.ai.ChatMessage[],
}

class s48_ContinueInput {
  session_id: string,
  message: string,
}

class s48_ContinueOutput {
  session_id: string,
  reply: string,
  transcript: s48_Transcript,
}

function s48_prompt_template(
  text: string,
) -> (baml.llm.Context) -> baml.llm.PromptAst throws never {
  (ctx: baml.llm.Context) -> baml.llm.PromptAst {
    baml.llm.assemble_prompt_ast(["", ""], [text])
  }
}

// Deterministic provider for the HTTP reference test. It implements the v2
// semantic capability directly and has no dependency on the old stdlib provider
// interfaces.
class s48_EchoProvider {
  reply: string,

  implements root.ai.Provider {}

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let value: T = baml.sap.parse<T>(self.reply) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["fake response did not match the requested output type"],
        },
      };
      root.ai.Response<T> {
        value: value,
        meta: root.ai.Meta {
          provider: "s48-echo",
          model: "fake",
          request_id: null,
          finish_reason: "stop",
          usage: null,
          attributes: {},
          raw: null,
        },
      }
    }
  }
}

// The declarative task remains the design-time source for the prompt and output
// contract. This phase does not rely on compiler desugaring when it is called.
// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s48_ContinueConversation(history_json: string, message: string) -> string {
//   client: s48_EchoProvider { reply: `"ack"` }
//   prompt `
//     ${role("system")}
//     Continue the conversation. Be concise and consistent with prior turns.
//     Prior transcript (JSON): ${history_json}
// 
//     ${role("user")}
//     ${message}
//   `
// }

// Hand-written reference expansion of the compiler task surface for
// `s48_ContinueConversation`; these helper names are scaffolding, not literal
// generated names:
// - `s48_ContinueConversation_request` represents
//   `s48_ContinueConversation.request(...)`.
// - `s48_ContinueConversation_desugared` spells the plain task call through
//   `root.ai.run`; its `_desugared` suffix exists only in this reference fake.
// Tools are intentionally absent; a caller/provider can add them later.
function s48_ContinueConversation_request(
  history_json: string,
  message: string,
  provider: root.ai.Provider = s48_EchoProvider { reply: `"ack"` },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s48_ContinueConversation",
    "ns_ai_scenarios2",
    { "history_json": history_json, "message": message },
    [],
    root.ai.RequestOptions {},
    {},
    s48_prompt_template(
      `Continue the conversation consistently. Prior transcript: ${history_json}\nUser: ${message}`,
    ),
  )
}

function s48_ContinueConversation_desugared(
  history_json: string,
  message: string,
  provider: root.ai.Provider = s48_EchoProvider { reply: `"ack"` },
) -> string {
  root.ai.run<string>(s48_ContinueConversation_request(history_json, message, provider = provider))
}

// A tiny in-process reference store. It stores JSON rather than a Request or a
// provider so the same shape can be replaced by Redis/Postgres without changing
// the handler contract.
class s48_AppHistoryStore {
  sessions: map<string, string>,

  function load(self, session_id: string) -> s48_Transcript {
    match (self.sessions.get(session_id)) {
      null => s48_Transcript { turns: [] },
      let raw: string => baml.json.from_string<s48_Transcript>(raw) catch_all (e) {
        _ => s48_Transcript { turns: [] },
      },
    }
  }

  function save(self, session_id: string, transcript: s48_Transcript) -> null {
    let _ = self.sessions.set(session_id, baml.json.to_string(transcript));
    null
  }
}

class s48_AppHistoryService {
  store: s48_AppHistoryStore,
  provider: root.ai.Provider,

  function process(self, input: s48_ContinueInput) -> s48_ContinueOutput {
    let transcript = self.store.load(input.session_id);
    let history_json = baml.json.to_string(transcript);
    let reply = s48_ContinueConversation_desugared(history_json, input.message, provider = self.provider);

    let turns = transcript.turns;
    let _ = turns.push(root.ai.ChatMessage.user(input.message));
    let _ = turns.push(root.ai.ChatMessage.assistant(reply));
    let updated = s48_Transcript { turns: turns };
    self.store.save(input.session_id, updated);

    s48_ContinueOutput {
      session_id: input.session_id,
      reply: reply,
      transcript: updated,
    }
  }

  // `Server.serve` requires a never-throwing handler, so errors become an HTTP
  // response at this boundary rather than escaping the server task.
  function handle(self, req: baml.http.Request) -> baml.http.Response {
    let body = baml.json.to_string(
      self.process(baml.json.from_string<s48_ContinueInput>(req.body)),
    ) catch_all (e) {
      _ => "{\"error\":\"conversation request failed\"}",
    };
    baml.http.Response.new(200, { "content-type": "application/json" }, body.to_utf8())
  }
}

function s48_call_server(addr: string, input: s48_ContinueInput) -> s48_ContinueOutput {
  let response = baml.http.send(baml.http.Request {
    method: "POST",
    url: "http://" + addr + "/continue",
    headers: { "content-type": "application/json" },
    body: baml.json.to_string(input),
  });
  baml.json.from_string<s48_ContinueOutput>(response.text())
}

// An executable two-request reference. It intentionally lives in an integ
// testset so `baml check` compiles it while the current phase does not run it.
testset "integ-test-48-http-app-history" {
  test "48: a second request loads the transcript saved by the first" {
    let service = s48_AppHistoryService {
      store: s48_AppHistoryStore { sessions: {} },
      provider: s48_EchoProvider { reply: `"ack"` },
    };
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn { server.serve(service.handle) };

    let _first = s48_call_server(server.addr, s48_ContinueInput {
      session_id: "demo-session",
      message: "My favorite color is green.",
    });
    let second = s48_call_server(server.addr, s48_ContinueInput {
      session_id: "demo-session",
      message: "What is my favorite color?",
    });
    task.cancel();

    assert.equal(second.transcript.turns.length(), 4)
  }
}
