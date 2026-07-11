// Task sketch: s19_ContinueBranch(history, continuation) -> string.
// Its complete hand-written lowering is the request builder and driver below.

// Manual expansion reference for task `s19_ContinueBranch`:
// - `s19_ContinueBranch_request(...)` models
//   `s19_ContinueBranch.request(...)`.
// - `s19_ContinueBranch_manual(...)` models the plain
//   `s19_ContinueBranch(...)` call, lowered through `root.ai.run`.
// - A session-bound call below models
//   `session.run(s19_ContinueBranch.request(...))`; forking is a Session
//   resource operation, not a task modifier.
// The branch helpers are ordinary application orchestration. All helper names
// here are hand-written reference/test scaffolding, not literal
// compiler-generated names.
function s19_ContinueBranch_request(
  history: string,
  continuation: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "s19-default", reply: `"unused"`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s19_ContinueBranch", "ns_ai_scenarios",
    { "history": history, "continuation": continuation },
    [], root.ai.RequestOptions {}, {},
    bepv2_prompt_template(`Conversation so far:\n${history}\n\nContinue with: ${continuation}`, false),
  )
}

function s19_ContinueBranch_manual(
  history: string,
  continuation: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "s19-default", reply: `"unused"`, attributes: {} },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s19_ContinueBranch_request(history, continuation, provider = provider))
}

function s19_run_branch(
  provider: root.ai.Provider,
  prefix: root.ai.ChatMessage[],
  id: string,
  continuation: string,
) -> s19_Branch throws baml.errors.CallError | baml.errors.UnknownError {
  let messages = s19_fork(prefix, continuation);
  let history = prefix.map((message) -> { `${message.role}: ${s19_text_of(message)}` }).join("\n");
  let reply = s19_ContinueBranch_manual(history, continuation, provider = provider);
  s19_Branch { id: id, messages: messages, reply: reply }
}

function s19_explore3(
  provider: root.ai.Provider,
  prefix: root.ai.ChatMessage[],
  a: string,
  b: string,
  c: string,
) -> s19_Branch[] throws baml.errors.CallError | baml.errors.UnknownError {
  let future_a = spawn { s19_run_branch(provider, prefix, "a", a) };
  let future_b = spawn { s19_run_branch(provider, prefix, "b", b) };
  let future_c = spawn { s19_run_branch(provider, prefix, "c", c) };
  [await future_a, await future_b, await future_c]
}

function s19_pick_winner(branches: s19_Branch[], want: string) -> s19_Branch throws never {
  for (let branch in branches) {
    if (branch.reply.includes(want)) { return branch; }
  }
  branches.at(0) ?? s19_Branch { id: "none", messages: [], reply: "" }
}

test "19: forking at the tail leaves the original trunk untouched" {
  let prefix = [
    root.ai.ChatMessage.system("You are a copywriter."),
    root.ai.ChatMessage.user("Draft a tagline for a coffee brand."),
  ];
  let casual = s19_run_branch(s19_ForkEcho {}, prefix, "casual", "Keep it playful.");
  let formal = s19_run_branch(s19_ForkEcho {}, prefix, "formal", "Rewrite it formally.");
  assert.contains(casual.reply, "Keep it playful")
  assert.contains(formal.reply, "Rewrite it formally")
  assert.equal(prefix.length(), 2)
  assert.equal(casual.messages.length(), 3)
}

test "19: forking anywhere drops later turns from the branch, not the trunk" {
  let history = [
    root.ai.ChatMessage.system("You are a copywriter."),
    root.ai.ChatMessage.user("Draft a tagline."),
    root.ai.ChatMessage.assistant("Fuel your morning."),
    root.ai.ChatMessage.user("Add a call to action."),
    root.ai.ChatMessage.assistant("Order now."),
  ];
  let branch = s19_fork_at(history, 3, "Keep it understated.");
  assert.equal(branch.length(), 4)
  assert.is_true(!branch.map((message) -> { s19_text_of(message) }).join(" ").includes("call to action"))
  assert.equal(history.length(), 5)
}

test "19: fan out three branches from a shared prefix and pick a winner" {
  let prefix = [root.ai.ChatMessage.user("Name a note-taking app.")];
  let branches = s19_explore3(
    s19_ForkEcho {}, prefix, "Make it playful.", "Make it premium.", "Make it technical.",
  );
  assert.equal(branches.length(), 3)
  assert.equal(s19_pick_winner(branches, "premium").id, "b")
}

test "19: a forkable session produces a distinct resource" {
  let session = root.ai.open_session(s19_SessionProvider {}, root.ai.SessionOptions {});
  let id = match (session) {
    let forkable: root.ai.ForkableSession => forkable.fork().token().id,
    _ => "not forkable",
  };
  assert.equal(id, "root-fork")
}

test "19: a plain session cannot be narrowed to ForkableSession" {
  let session: root.ai.Session = s19_PlainSession { owner: s19_ForkEcho {} };
  let label = match (session) {
    let forkable: root.ai.ForkableSession => "forkable",
    _ => "plain",
  };
  assert.equal(label, "plain")
}

testset "integ-test-19-fork-branch" {
  test "19 live: two extends from the same chain handle are independent branches" {
    let provider: root.ai.Provider = root.ai.OpenAiResponses {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    };
    let shared = root.ai.open_session(provider, root.ai.SessionOptions {});
    let _ = shared.run<string>(s19_ContinueBranch_request("", "My favorite number is 7. Reply only: ok", provider = provider));
    let answers = match (shared) {
      let forkable: root.ai.ForkableSession => {
        let a = forkable.fork();
        let b = forkable.fork();
        [
          a.run<string>(s19_ContinueBranch_request("", "Is it even or odd? One word.", provider = provider)),
          b.run<string>(s19_ContinueBranch_request("", "What is it times two? One number.", provider = provider)),
        ]
      },
      _ => ["provider session is not forkable", "provider session is not forkable"],
    };
    assert.is_true(answers[0].to_lower_case().includes("odd"))
    assert.contains(answers[1], "14")
    shared.cleanup();
  }
}
