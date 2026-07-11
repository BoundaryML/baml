// Task sketch: s20_ContinueChain(followup) -> string.
// Its complete hand-written lowering is the request builder and driver below.

// Manual expansion reference for task `s20_ContinueChain`:
// - `s20_ContinueChain_request(...)` models
//   `s20_ContinueChain.request(...)`.
// - `s20_ContinueChain_manual(...)` models the plain
//   `s20_ContinueChain(...)` call, lowered through `root.ai.run`.
// - Calls below of the form `session.run(s20_ContinueChain_request(...))`
//   model `session.run(s20_ContinueChain.request(...))`; session open, resume,
//   fork, and cleanup are resource operations rather than task modifiers.
// These helper names are hand-written reference/test scaffolding, not literal
// compiler-generated names.
function s20_ContinueChain_request(
  followup: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "s20-default", reply: `"unused"`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s20_ContinueChain", "ns_ai_scenarios", { "followup": followup },
    [], root.ai.RequestOptions {}, {}, bepv2_prompt_template(followup, false),
  )
}

function s20_ContinueChain_manual(
  followup: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "s20-default", reply: `"unused"`, attributes: {} },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s20_ContinueChain_request(followup, provider = provider))
}

function s20_resume_label(
  provider: s20_ScriptedSessions,
  token: root.ai.SessionToken,
) -> string throws never {
  let session = provider.resume_session(token) catch (e) {
    let u: baml.errors.UnknownError => return u.message.join(" "),
  };
  session.cleanup();
  "accepted"
}

test "20: session.run sends only the new request while the resource retains context" {
  let provider = s20_ScriptedSessions { owner: "scripted" };
  let session = root.ai.open_session(provider, root.ai.SessionOptions {});
  let _ = session.run<string>(s20_ContinueChain_request("The capital of France is Paris."));
  let reply = session.run<string>(s20_ContinueChain_request("What was the capital?"));
  assert.contains(reply, "Paris")
  assert.contains(reply, "What was the capital")
}

test "20: a foreign-owner token is refused" {
  let provider = s20_ScriptedSessions { owner: "scripted" };
  let label = s20_resume_label(provider, root.ai.SessionToken { provider: "other", id: "x" });
  assert.contains(label, "foreign session token")
}

test "20: a SessionToken survives a string round trip" {
  let provider = s20_ScriptedSessions { owner: "scripted" };
  let session = root.ai.open_session(provider, root.ai.SessionOptions {});
  let _ = session.run<string>(s20_ContinueChain_request("secret code SPARROW-7"));
  let saved = baml.json.to_string<root.ai.SessionToken>(session.token());
  let token = baml.json.from_string<root.ai.SessionToken>(saved);
  let resumed = provider.resume_session(token);
  let reply = resumed.run<string>(s20_ContinueChain_request("What was the code?"));
  assert.contains(reply, "SPARROW-7")
}

test "20: two forks from one session share the root and not each other" {
  let session = root.ai.open_session(s20_ScriptedSessions { owner: "scripted" }, root.ai.SessionOptions {});
  let _ = session.run<string>(s20_ContinueChain_request("root fact: the sky is blue"));
  let replies = match (session) {
    let forkable: root.ai.ForkableSession => {
      let a = forkable.fork();
      let b = forkable.fork();
      [
        a.run<string>(s20_ContinueChain_request("branch A question")),
        b.run<string>(s20_ContinueChain_request("branch B question")),
      ]
    },
    _ => ["", ""],
  };
  assert.contains(replies[0], "the sky is blue")
  assert.is_true(!replies[0].includes("branch B"))
  assert.contains(replies[1], "branch B question")
}

test "20: a plain provider does not negotiate Sessions" {
  let provider: root.ai.Provider = root.ai.OpenAi {
    model: "gpt-5.6-luna", api_key: "unused", base_url: null,
  };
  let label = match (provider) {
    let sessions: root.ai.Sessions => "sessions",
    _ => "plain",
  };
  assert.equal(label, "plain")
}

testset "integ-test-20-server-chains" {
  test "20 live: extend recalls a fact across the server-stored chain" {
    let provider: root.ai.Provider = root.ai.OpenAiResponses {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    };
    let session = root.ai.open_session(provider, root.ai.SessionOptions {});
    let _ = session.run<string>(s20_ContinueChain_request("My favorite number is 41. Remember it. Reply: ok", provider = provider));
    let answer = session.run<string>(s20_ContinueChain_request("What is my favorite number? Just the number.", provider = provider));
    assert.contains(answer, "41")
    session.cleanup();
  }

  test "20 live: two extends fork from one handle, both recalling the shared root" {
    let provider: root.ai.Provider = root.ai.OpenAiResponses {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    };
    let root_session = root.ai.open_session(provider, root.ai.SessionOptions {});
    let _ = root_session.run<string>(s20_ContinueChain_request("Remember: the magic word is ORANGE. Reply: ok", provider = provider));
    let replies = match (root_session) {
      let forkable: root.ai.ForkableSession => {
        let a = forkable.fork();
        let b = forkable.fork();
        [
          a.run<string>(s20_ContinueChain_request("What is the magic word?", provider = provider)),
          b.run<string>(s20_ContinueChain_request("State the magic word in lowercase.", provider = provider)),
        ]
      },
      _ => ["", ""],
    };
    assert.contains(replies[0].to_upper_case(), "ORANGE")
    assert.contains(replies[1].to_upper_case(), "ORANGE")
    root_session.cleanup();
  }
}
