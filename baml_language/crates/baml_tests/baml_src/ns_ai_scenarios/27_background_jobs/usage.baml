// This is the desugared code for s27_ReviewJob ----------------------------
function s27_ReviewJob_request(
  topic: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "s27-default", reply: `"unused"`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s27_ReviewJob", "ns_ai_scenarios", { "topic": topic },
    [], root.ai.RequestOptions {}, {},
    bepv2_prompt_template(`Review ${topic} and return a concise result.`, false),
  )
}

function s27_ReviewJob_manual(
  topic: string,
  provider: root.ai.Provider,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s27_ReviewJob_request(topic, provider = provider))
}

function s27_ReviewJob_background_manual(
  topic: string,
  idempotency_key: string,
  provider: root.ai.Provider,
) -> root.ai.Job<string> throws baml.errors.UnknownError {
  root.ai.submit_background<string>(
    s27_ReviewJob_request(topic, provider = provider),
    root.ai.BackgroundOptions { idempotency_key: idempotency_key },
  )
}

function s27_resume_label(
  provider: s27_ScriptedJobs,
  token: root.ai.JobToken,
) -> string throws never {
  let job = provider.resume_job<string>(token) catch (e) {
    let u: baml.errors.UnknownError => return u.message.join(" "),
  };
  job.token().id
}

function s27_background_support_label(provider: root.ai.Provider) -> string throws never {
  let job = s27_ReviewJob_background_manual("x", "key", provider) catch (e) {
    let u: baml.errors.UnknownError => return "unsupported",
  };
  job.cleanup();
  "supported"
}

test "27: submit returns a handle now; polling goes pending then done" {
  let provider = s27_ScriptedJobs { owner: "scripted", remaining_polls: 1 };
  let job = s27_ReviewJob_background_manual("architecture", "req-42", provider);
  assert.equal(s27_poll_label(job), "pending")
  assert.equal(s27_poll_label(job), "done job-req-42")
}

test "27: the app-owned idempotency key deduplicates job identity" {
  let provider = s27_ScriptedJobs { owner: "scripted", remaining_polls: 0 };
  let a = s27_ReviewJob_background_manual("review", "key-A", provider);
  let retry = s27_ReviewJob_background_manual("review again", "key-A", provider);
  let b = s27_ReviewJob_background_manual("review", "key-B", provider);
  assert.equal(a.token().id, retry.token().id)
  assert.is_true(a.token().id != b.token().id)
}

test "27: a JobToken survives a string round trip and resumes" {
  let provider = s27_ScriptedJobs { owner: "scripted", remaining_polls: 0 };
  let job = s27_ReviewJob_background_manual("long job", "persist-me", provider);
  let saved = baml.json.to_string<root.ai.JobToken>(job.token());
  let token = baml.sap.parse<root.ai.JobToken>(saved);
  let resumed = provider.resume_job<string>(token);
  assert.equal(s27_poll_label(resumed), "done job-persist-me")
}

test "27: a foreign-owner token is refused at resume" {
  let provider = s27_ScriptedJobs { owner: "scripted", remaining_polls: 0 };
  let label = s27_resume_label(provider, root.ai.JobToken {
    provider: "openai-responses", id: "resp_xyz",
  });
  assert.contains(label, "different provider")
}

test "27: wrapping a provider for Generate does not re-drive Background effects" {
  let base: root.ai.Provider = s27_ScriptedJobs { owner: "scripted", remaining_polls: 0 };
  let guarded: root.ai.Provider = base.with_retry(3);
  let label = match (guarded) {
    let background: root.ai.Background => "background",
    _ => "not background",
  };
  assert.equal(label, "not background")
}

test "27: a non-background provider is rejected by the background driver" {
  let label = s27_background_support_label(root.ai.OpenAi {
    model: "gpt-5.6-luna", api_key: "unused", base_url: null,
  });
  assert.equal(label, "unsupported")
}

testset "integ-test-27-background-jobs" {
  test "27 live: submit a background job, then poll until it lands" {
    let provider: root.ai.Provider = root.ai.OpenAiResponses {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    };
    let job = s27_ReviewJob_background_manual(
      "Reply with exactly one word: pong", baml.id.new(), provider,
    );
    assert.is_true(job.token().id != "")

    let landed = "";
    let attempts = 0;
    while (attempts < 25 && landed == "") {
      match (job.poll()) {
        let pending: root.ai.Pending => {
          baml.sys.sleep(pending.retry_after ?? baml.time.Duration.from_seconds(2)) catch (e) { _ => null };
          attempts = attempts + 1;
        },
        let done: root.ai.Done<string> => { landed = done.value; },
        let failed: root.ai.Failed => { attempts = 25; },
        let cancelled: root.ai.Cancelled => { attempts = 25; },
      }
    }
    assert.contains(landed.to_lower_case(), "pong")
    job.cleanup();
  }
}
