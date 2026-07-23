// Scenario 50 — provider-owned background work across HTTP requests.
//
// Submission returns a process-local Job<T>; only JobToken crosses HTTP. Each
// poll request explicitly resumes a new local Job<T> on the configured provider.

class s50_ResearchInput {
  token: root.ai.JobToken?,
  topic: string,
}

class s50_ResearchOutput {
  token: root.ai.JobToken,
  status: string,
  result: string?,
}

function s50_prompt_template(
  text: string,
) -> (baml.llm.Context) -> baml.llm.PromptAst throws never {
  (ctx: baml.llm.Context) -> baml.llm.PromptAst {
    baml.llm.assemble_prompt_ast(["", ""], [text])
  }
}

class s50_JobState {
  polls: int,
  cancelled: bool,
  result_json: string,
}

class s50_BackgroundProvider {
  jobs: map<string, s50_JobState>,
  idempotency_keys: map<string, string>,
  next_id: int,
  result_json: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s50-background-provider" }
  }

  implements root.ai.Background {
    function submit<T>(
      self,
      request: root.ai.Request<T>,
      options: root.ai.BackgroundOptions,
    ) -> root.ai.Job<T> throws baml.errors.UnknownError {
      let existing = match (options.idempotency_key) {
        let key: string => self.idempotency_keys.get(key),
        null => null,
      };
      let id = match (existing) {
        let found: string => found,
        null => {
          let fresh = `job-${self.next_id}`;
          self.next_id = self.next_id + 1;
          let _ = self.jobs.set(fresh, s50_JobState {
            polls: 0,
            cancelled: false,
            result_json: self.result_json,
          });
          match (options.idempotency_key) {
            let key: string => { let _ = self.idempotency_keys.set(key, fresh); },
            null => {},
          }
          fresh
        },
      };
      s50_Job<T> { owner: self, id: id }
    }

    function resume_job<T>(self, token: root.ai.JobToken) -> root.ai.Job<T>
        throws baml.errors.UnknownError {
      if (token.provider != self.display_name() || self.jobs.get(token.id) == null) {
        throw baml.errors.UnknownError {
          data: token,
          message: ["job token does not belong to this configured provider"],
        };
      }
      s50_Job<T> { owner: self, id: token.id }
    }
  }
}

class s50_Job<T> {
  owner: s50_BackgroundProvider,
  id: string,

  implements root.ai.Job<T> {
    function poll(self) -> root.ai.Done<T> | root.ai.Pending | root.ai.Failed | root.ai.Cancelled
        throws baml.errors.UnknownError {
      let state = match (self.owner.jobs.get(self.id)) {
        let value: s50_JobState => value,
        null => throw baml.errors.UnknownError { data: self.id, message: ["unknown job"] },
      };
      if (state.cancelled) { return root.ai.Cancelled {}; }
      state.polls = state.polls + 1;
      if (state.polls < 2) {
        return root.ai.Pending { retry_after: baml.time.Duration.from_milliseconds(1n) };
      }
      let value: T = baml.json.from_string<T>(state.result_json) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["background result parse failed"] },
      };
      root.ai.Done<T> {
        value: value,
        meta: root.ai.Meta {
          provider: self.owner.display_name(),
          model: "fake-background",
          request_id: self.id,
          finish_reason: "stop",
          usage: null,
          attributes: {},
          raw: null,
        },
      }
    }

    function cancel(self) -> null throws baml.errors.UnknownError {
      match (self.owner.jobs.get(self.id)) {
        let state: s50_JobState => { state.cancelled = true; },
        null => throw baml.errors.UnknownError { data: self.id, message: ["unknown job"] },
      }
      null
    }

    function token(self) -> root.ai.JobToken throws never {
      root.ai.JobToken { provider: self.owner.display_name(), id: self.id }
    }

    function cleanup(self) -> null throws never { null }
  }
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s50_DeepResearch(topic: string) -> string {
//   client: s50_BackgroundProvider {
//     jobs: {}, idempotency_keys: {}, next_id: 0, result_json: `"research complete"`,
//   }
//   prompt: `Research ${topic} thoroughly.`
// }

// Hand-written reference expansion of the compiler task surface for
// `s50_DeepResearch`; the helper name is scaffolding, not a literal generated name:
// - `s50_DeepResearch_request` represents `s50_DeepResearch.request(...)`.
// `s50_BackgroundService.process` supplies the application-level background
// submit/resume lifecycle; it is not a compiler-generated helper.
function s50_DeepResearch_request(
  topic: string,
  provider: root.ai.Provider = s50_BackgroundProvider {
    jobs: {}, idempotency_keys: {}, next_id: 0, result_json: `"research complete"`,
  },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s50_DeepResearch",
    "ns_ai_scenarios2",
    { "topic": topic },
    [],
    root.ai.RequestOptions {},
    {},
    s50_prompt_template(`Research ${topic} thoroughly.`),
  )
}

function s50_resume_job(
  provider: root.ai.Provider,
  token: root.ai.JobToken,
) -> root.ai.Job<string> throws baml.errors.UnknownError {
  let background = match (provider) {
    let capability: root.ai.Background => capability,
    _ => throw baml.errors.UnknownError {
      data: provider,
      message: ["configured provider cannot resume background jobs"],
    },
  };
  background.resume_job<string>(token)
}

class s50_BackgroundService {
  provider: root.ai.Provider,

  function process(self, input: s50_ResearchInput) -> s50_ResearchOutput
      throws baml.errors.UnknownError {
    match (input.token) {
      null => {
        let job = root.ai.submit_background<string>(
          s50_DeepResearch_request(input.topic, provider = self.provider),
          root.ai.BackgroundOptions { idempotency_key: "research-" + input.topic },
        );
        defer { job.cleanup() }
        s50_ResearchOutput { token: job.token(), status: "submitted", result: null }
      },
      let token: root.ai.JobToken => {
        let job = s50_resume_job(self.provider, token);
        defer { job.cleanup() }
        match (job.poll()) {
          let done: root.ai.Done<string> => s50_ResearchOutput {
            token: token, status: "done", result: done.value,
          },
          let ignored: root.ai.Pending => s50_ResearchOutput {
            token: token, status: "pending", result: null,
          },
          let ignored: root.ai.Failed => s50_ResearchOutput {
            token: token, status: "failed", result: null,
          },
          let ignored: root.ai.Cancelled => s50_ResearchOutput {
            token: token, status: "cancelled", result: null,
          },
        }
      },
    }
  }

  function handle(self, req: baml.http.Request) -> baml.http.Response {
    let body = baml.json.to_string(
      self.process(baml.json.from_string<s50_ResearchInput>(req.body)),
    ) catch_all (e) {
      _ => "{\"error\":\"background request failed\"}",
    };
    baml.http.Response.new(200, { "content-type": "application/json" }, body.to_utf8())
  }
}

function s50_call_server(addr: string, input: s50_ResearchInput) -> s50_ResearchOutput {
  let response = baml.http.send(baml.http.Request {
    method: "POST",
    url: "http://" + addr + "/research",
    headers: { "content-type": "application/json" },
    body: baml.json.to_string(input),
  });
  baml.json.from_string<s50_ResearchOutput>(response.text())
}

testset "integ-test-50-http-background-job" {
  test "50: later HTTP requests resume and poll using JobToken only" {
    let configured = s50_BackgroundProvider {
      jobs: {},
      idempotency_keys: {},
      next_id: 0,
      result_json: `"research complete"`,
    };
    let service = s50_BackgroundService { provider: configured };
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn { server.serve(service.handle) };

    let submitted = s50_call_server(server.addr, s50_ResearchInput {
      token: null,
      topic: "BEPv2 resources",
    });
    let pending = s50_call_server(server.addr, s50_ResearchInput {
      token: submitted.token,
      topic: "BEPv2 resources",
    });
    let done = s50_call_server(server.addr, s50_ResearchInput {
      token: submitted.token,
      topic: "BEPv2 resources",
    });
    task.cancel();

    assert.equal(submitted.status, "submitted")
    assert.equal(pending.status, "pending")
    assert.equal(done.status, "done")
    assert.equal(done.result, "research complete")
  }
}
