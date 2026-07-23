// 17 — application-held history is ChatMessage[] threaded through an ordinary task.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s17_Chat(history: root.ai.ChatMessage[], latest: string) -> string {
//   client: root.ai.FakeGenerateProvider { name: "chat-default", reply: `"Fred"`, attributes: {} }
//   prompt: `Conversation so far:\n${root.ai.render_messages(history)}\nuser: ${latest}`
// }

// Manual expansion reference for task `s17_Chat`:
// - `s17_Chat_request(...)` models `s17_Chat.request(...)`.
// - `s17_Chat_manual(...)` models the plain `s17_Chat(...)` call, lowered
//   through `root.ai.run`.
// - `s17_two_turn(...)` is ordinary application orchestration over two task
//   calls; it is not a synthesized task modifier.
// These helper names are hand-written reference/test scaffolding, not literal
// compiler-generated names.
function s17_Chat_request(
  history: root.ai.ChatMessage[],
  latest: string,
  provider: root.ai.Provider = root.ai.FakeGenerateProvider {
    name: "chat-default", reply: `"Fred"`, attributes: {},
  },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s17_Chat",
    "ns_ai_scenarios",
    { "history": history, "latest": latest },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(
      `Conversation so far:\n${root.ai.render_messages(history)}\nuser: ${latest}`,
      false,
    ),
  )
}

function s17_Chat_manual(
  history: root.ai.ChatMessage[],
  latest: string,
  provider: root.ai.Provider = root.ai.FakeGenerateProvider {
    name: "chat-default", reply: `"Fred"`, attributes: {},
  },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s17_Chat_request(history, latest, provider = provider))
}

function s17_two_turn(
  provider: root.ai.Provider,
  first: string,
  second: string,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  let first_answer = s17_Chat_manual([], first, provider = provider);
  let history = [
    root.ai.ChatMessage.user(first),
    root.ai.ChatMessage.assistant(first_answer),
  ];
  s17_Chat_manual(history, second, provider = provider)
}

test "17: the second request contains application-owned history" {
  let history = [
    root.ai.ChatMessage.user("My name is Fred"),
    root.ai.ChatMessage.assistant("ok"),
  ];
  let request = s17_Chat_request(history, "What is my name?");
  assert.is_true((request.messages().at(0) ?? root.ai.ChatMessage.user("missing")).text().includes("Fred"))
}

test "17: the app can serialize its history between requests" {
  let history = [root.ai.ChatMessage.user("remember this")];
  let encoded = baml.json.to_string(history);
  let decoded = baml.json.from_string<root.ai.ChatMessage[]>(encoded);
  assert.equal((decoded.at(0) ?? root.ai.ChatMessage.user("missing")).text(), "remember this")
}

testset "integ-test-17-history-sessions" {
  test "17 live: a threaded transcript remembers across turns" {
    let live: root.ai.Provider = root.ai.OpenAi {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
    };
    let r = s17_two_turn(
      live,
      "My name is Fred. Reply with just: ok",
      "What is my name? Reply with just my name.",
    ) catch (e) { _ => "live call failed" };
    assert.is_true(r.to_lower_case().includes("fred"))
  }
}
