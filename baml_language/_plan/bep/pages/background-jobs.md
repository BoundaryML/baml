# Background Jobs End to End

This page follows one task from source code to a remote job and back. It is intentionally detailed because background work combines prompt rendering, typed parsing, external effects, persistence, polling, and cleanup.

## The application requirement

We want to review a repository. The model may take several minutes, so the API returns immediately with a remote job. A worker polls later and eventually obtains a typed `RepositoryReview`.

```baml
class RepositoryReview {
  summary: string,
  architecture: string[],
  risks: string[],
  suggested_changes: string[],
}

function ReviewRepository(repo_url: string) -> RepositoryReview {
  client: LongRunningModel
  prompt: `
    ${role("system")}
    You are a senior software reviewer.

    ${role("user")}
    Review ${repo_url}.
    ${ctx.output_format}
  `
}
```

This is still an LLM function. “Background” is how this invocation executes, not what the task means.

## Submission

```baml
let job = baml.ai.submit_background(
  ReviewRepository$request(
    "https://github.com/boundaryml/baml",
    client = LongRunningModel,
  ),
  baml.ai.BackgroundOptions {
    idempotency_key: "repo-review-boundaryml-baml-2026-07-09",
  },
)
```

The types are:

```text
ReviewRepository$request(...) -> LlmRequest<RepositoryReview>
submit_background(...)        -> Job<RepositoryReview>
job.poll()                     -> JobPending | JobSucceeded<RepositoryReview> | JobFailed | JobCancelled
```

The output type survives the entire asynchronous lifecycle.

## The standard capability

```baml
class BackgroundOptions {
  idempotency_key: string,
  tags: map<string, string> = {},
}

interface Background requires Provider {
  function submit<T>(
    self,
    request: LlmRequest<T>,
    options: BackgroundOptions,
  ) -> Job<T> throws baml.errors.BackgroundError | baml.errors.UnknownError

  function resume<T>(
    self,
    token: JobToken,
  ) -> Job<T> throws baml.errors.BackgroundError | baml.errors.UnknownError
}
```

The provider capability submits or resumes. Polling belongs to the returned job resource because the job now owns the remote identifier and parser state.

## The driver

```baml
function submit_background<T>(
  request: LlmRequest<T>,
  options: BackgroundOptions,
) -> Job<T> {
  match (request.provider) {
    let provider: Background => provider.submit<T>(request, options),
    _ => throw baml.errors.Unsupported {
      capability: "baml.ai.Background",
      provider: request.provider_name(),
    },
  }
}
```

This is ordinary BAML. The compiler only generated `ReviewRepository$request`; it did not generate a task-specific background companion.

## The job protocol

```baml
enum JobPhase {
  Queued,
  Running,
  Succeeded,
  Failed,
  Cancelling,
  Cancelled,
  Expired,
}

class JobPending {
  phase: JobPhase,
  retry_after: baml.time.Duration?,
}

class JobSucceeded<T> {
  value: T,
  meta: ResponseMeta,
}

class JobFailed {
  error: baml.errors.BackgroundError,
}

class JobCancelled {}

interface Job<T> {
  function status(self) -> JobPhase
    throws baml.errors.BackgroundError | baml.errors.UnknownError

  function poll(self) -> JobPending | JobSucceeded<T> | JobFailed | JobCancelled
    throws baml.errors.BackgroundError | baml.errors.UnknownError

  function cancel(self) -> JobPhase
    throws baml.errors.BackgroundError | baml.errors.UnknownError

  function token(self) -> JobToken throws never
  function cleanup(self) -> void
}
```

`poll()` returns a state sum rather than `T?`:

- `null` cannot distinguish queued, running, cancelling, or expired;
- a typed failure should not be confused with “not ready”;
- some providers return a suggested poll interval;
- cancellation may be asynchronous;
- the successful response still has metadata.

## A provider implementation

The following is illustrative pseudocode for an OpenAI Responses-style background provider.

```baml
class OpenAiResponses {
  model: string,
  api_key: string,
  base_url: string,

  implements Provider {}

  implements Background {
    function submit<T>(
      self,
      request: LlmRequest<T>,
      options: BackgroundOptions,
    ) -> Job<T> {
      let response = self.post_response<T>(
        request,
        background = true,
        idempotency_key = options.idempotency_key,
      )

      OpenAiResponseJob<T> {
        owner: self,
        response_id: response.id,
        last_phase: map_openai_status(response.status),
      }
    }

    function resume<T>(self, token: JobToken) -> Job<T> {
      let payload = OpenAiJobToken.decode(token)
      if (payload.provider_instance != self.instance_name()) {
        throw baml.errors.WrongResourceOwner {
          expected: self.instance_name(),
          actual: payload.provider_instance,
        }
      }

      OpenAiResponseJob<T> {
        owner: self,
        response_id: payload.response_id,
        last_phase: JobPhase.Queued,
      }
    }
  }
}
```

The remote ID never appears in application polling code.

## The provider-specific resource

```baml
class OpenAiResponseJob<T> {
  owner: OpenAiResponses,
  response_id: string,
  last_phase: JobPhase,

  implements Job<T> {
    function status(self) -> JobPhase {
      let wire = self.owner.retrieve_response(self.response_id)
      self.last_phase = map_openai_status(wire.status)
      self.last_phase
    }

    function poll(self) -> JobPending | JobSucceeded<T> | JobFailed | JobCancelled {
      let wire = self.owner.retrieve_response(self.response_id)
      self.last_phase = map_openai_status(wire.status)

      match (self.last_phase) {
        JobPhase.Queued | JobPhase.Running | JobPhase.Cancelling => {
          JobPending {
            phase: self.last_phase,
            retry_after: baml.time.Duration.from_seconds(2),
          }
        },
        JobPhase.Succeeded => {
          JobSucceeded<T> {
            value: self.owner.parse_response<T>(wire),
            meta: self.owner.parse_meta(wire),
          }
        },
        JobPhase.Cancelled => JobCancelled {},
        JobPhase.Failed | JobPhase.Expired => {
          JobFailed { error: self.owner.parse_background_error(wire) }
        },
      }
    }

    function cancel(self) -> JobPhase {
      let wire = self.owner.cancel_response(self.response_id)
      self.last_phase = map_openai_status(wire.status)
      self.last_phase
    }

    function token(self) -> JobToken {
      OpenAiJobToken {
        provider_instance: self.owner.instance_name(),
        response_id: self.response_id,
      }.encode()
    }

    function cleanup(self) -> void {
      // Release local polling/stream resources. Whether cleanup also cancels
      // remote work is an explicit provider option; cancel() is never implicit.
      self.owner.release_local_job(self.response_id) catch_all (e) { _ => null }
    }
  }
}
```

This is what “the resource encapsulates ownership, lifecycle, polling, and provider-specific identifiers” means:

- **ownership:** `owner` is the configured provider object allowed to operate on the response;
- **lifecycle:** the object exposes status transitions, cancellation, and cleanup;
- **polling:** it knows the retrieval endpoint and how vendor states map into the typed job-state union;
- **provider-specific identifiers:** `response_id` remains inside the implementation;
- **typed completion:** it retains `T` and knows how to parse the eventual response;
- **persistence:** it can emit a non-secret token and validate that token on resume.

## Polling in a worker

```baml
function wait_for_job<T>(job: baml.ai.Job<T>) -> T {
  while (true) {
    match (job.poll()) {
      baml.ai.JobPending { retry_after: let delay } => {
        baml.sys.sleep(delay ?? baml.time.Duration.from_seconds(2))
      },
      baml.ai.JobSucceeded<T> { value: let value } => return value,
      baml.ai.JobFailed { error: let error } => throw error,
      baml.ai.JobCancelled => throw baml.errors.Cancelled {
        message: "background job was cancelled",
      },
    }
  }
  baml.sys.panic("unreachable: job polling loop ended")
}
```

Polling policy may instead live in an application scheduler. The job deliberately exposes one non-blocking `poll` operation; a `wait` helper can be library code.

## Persistence and resume

Persist the token, expected output identity, and application idempotency record:

```baml
class SavedJob {
  task: string,
  output_schema_version: string,
  provider_instance: string,
  token: baml.ai.JobToken,
  idempotency_key: string,
}
```

The token MUST NOT contain credentials or a serialized provider object. The resuming process supplies the provider configuration again:

```baml
function resume_review(saved: SavedJob) -> RepositoryReview {
  if (saved.output_schema_version != REPOSITORY_REVIEW_SCHEMA_VERSION) {
    throw SchemaChanged { expected: saved.output_schema_version }
  }

  let job = LongRunningModel.resume_job<RepositoryReview>(saved.token)
  wait_for_job(job)
}
```

The exact schema-identity API is outside this BEP, but a durable workflow SHOULD guard against parsing an old job with an incompatible new `T`.

## Idempotency and duplicate submission

Submission is the dangerous transition. A transport timeout does not prove whether the remote service created the job.

The driver therefore declares:

```text
replay policy = RequiresIdempotencyKey(key)
```

The provider MUST forward the key using the vendor's supported mechanism or reject automatic retries. If the vendor has no idempotency support, a timeout after submission has `commit_state = Unknown`, and retry/fallback MUST stop rather than create a likely duplicate.

Polling is normally replay-safe. Cancellation may be idempotent but is not assumed to be unless the provider declares it.

## Cancellation and cleanup are different

`cancel()` requests a remote state transition. It may fail, race with completion, or return `Cancelling`.

`cleanup()` releases local resources and runs at most once under BAML cleanup semantics. It MUST NOT silently cancel paid remote work unless the resource type documents that behavior and the caller opted into it.

Use explicit cancellation when the application means to stop work:

```baml
defer { job.cleanup() }

if (request_was_withdrawn()) {
  let phase = job.cancel()
  log.info(`cancel phase=${phase}`)
}
```

## Why there is no background LLM-function DSL keyword

This BEP does not add:

```baml
function ReviewRepository(...) -> RepositoryReview {
  background: true
  ...
}
```

The same task may run immediately in one call, stream in another, and run in the background in a third. Execution mode belongs at the call site.

Nor does it require a generated `ReviewRepository$background` companion. The explicit form is short, works for user-defined capabilities, and keeps compiler output bounded:

```baml
baml.ai.submit_background(ReviewRepository$request(repo), options)
```

## Under-the-hood expansion

The compiler sees:

```baml
ReviewRepository$request(repo, client = LongRunningModel)
```

and builds:

```baml
baml.ai.LlmRequest<RepositoryReview> {
  provider: LongRunningModel,
  prompt: prompt`
    ${role("system")}You are a senior software reviewer.
    ${role("user")}Review ${repo}.
    ${ctx.output_format}
  `(baml.ai.build_prompt_context<RepositoryReview>(LongRunningModel)),
  identity: baml.ai.LlmFunctionIdentity {
    package: "app",
    name: "ReviewRepository",
  },
  arguments: { "repo_url": repo },
  options: {},
  tags: {},
}
```

`submit_background` then matches the request's provider, not the function declaration's original client. Client overrides therefore work consistently.

## Tests

The minimum test suite contains:

1. `$request` renders roles, media, and output format correctly.
2. The driver rejects a provider without `Background` using typed `Unsupported`.
3. Submit forwards one idempotency key.
4. A timeout with unknown commit state is not blindly retried.
5. Provider statuses map to every typed job-state arm.
6. Success SAP-parses the correct `T` and preserves metadata.
7. A token round-trips without credentials.
8. A token cannot resume on the wrong provider instance.
9. Cancellation handles already-completed and already-cancelled jobs.
10. Cleanup runs once and does not unexpectedly cancel remote work.
11. A live provider contract test submits, polls, and cleans up one inexpensive job.
