// ============================================================================
// usage.baml — APPLICATION-author code for background + poll jobs.
//
// Demonstrates:
//   - config-only `client`s, store:true baked in (so background is even legal)
//   - the .background companion: submit returns a Job handle IMMEDIATELY
//   - polling a persisted job AFTER a process restart (durability)
//   - a blocking wait_for with a Cadence (backoff) for callers that just want T
//   - TWO divergent providers (OpenAI inline-result, Anthropic batch second-fetch)
//     driven by byte-for-byte identical app code — portability proof
//   - reconciliation: the APP owns the idempotency key, threaded through Poller
//   - the honest failure when a non-Background provider is asked to .background
// ============================================================================


// ---- config-only clients (sugar for `function Name() -> Provider`) ----------

// store:true is REQUIRED for background; background() bakes it in. A ZDR shop
// literally cannot write this client (construction would refuse store:false).
client BigAnalysis() {
  OpenAIResponses.background("gpt-5")
}

// The divergent second provider. Same Background capability, batch-of-one wire.
client BatchClaude() {
  AnthropicBatch.default("claude-opus-4-1")
}

// A non-background provider, to show the .background companion's honest error
// (there is NO fallback — a one-shot call cannot fake a durable, resumable id).
client PlainGpt() {
  OpenAIResponses { model: "gpt-4o", api_key: env.OPENAI_API_KEY, store: false }
}


// ---- the target schema for the long job ------------------------------------

class ArchReview {
  summary: string
  risks: string[]
  score: int
}


// ============================================================================
// (A) The function. Written exactly like any other — the .background companion
//     is generated from it; the app author adds no job machinery to the function.
// ============================================================================

function ReviewRepo(repo: string) -> ArchReview {
  client: BigAnalysis()
  prompt #"Do a full architectural review of {{ repo }}. This may take minutes."#
}


// ============================================================================
// (B) Fire-and-forget: submit, persist the handle, return to the caller NOW.
//
//     This is the lifecycle that a plain blocking call cannot express. submit()
//     holds no connection; ReviewRepo.background returns a Job<ArchReview> whose
//     id we hand to our own durable store. The HTTP request that triggered THIS
//     function can return 202 to the user immediately.
// ============================================================================

function KickOffReview(repo: string, request_id: string) -> string
    throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
  // The APP owns the idempotency key — here, our own request id. If this endpoint
  // is retried by our load balancer, the same key reaches submit and the provider
  // dedupes instead of spawning a second multi-minute job (background §"reconcile").
  let job: Job<ArchReview> = ReviewRepo.background<ArchReview>(repo, request_id);
  // Persist the OPAQUE handle. After this we can crash, redeploy, scale to zero —
  // the job runs server-side and the id is all we need to come back.
  save_job_handle(request_id, job.id, job.owner, job.idempotency_key);
  job.id   // 202 Accepted; the client polls our /status/{job.id} later
}


// ============================================================================
// (C) Resume after a restart: re-hydrate a persisted handle and poll ONCE.
//
//     This runs in a DIFFERENT process from KickOffReview (a status endpoint, a
//     worker tick). poll() is a fresh GET by id — it needs nothing the submitting
//     process held. This is the entire durability claim, exercised.
// ============================================================================

function CheckReview(request_id: string) -> ReviewStatus
    throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
  let row = load_job_handle(request_id);
  // Rebuild the typed handle from our stored bytes. owner lets poll() refuse a
  // handle that doesn't belong to this provider.
  let job: Job<ArchReview> = Job<ArchReview> {
    id: row.id, owner: row.owner, idempotency_key: row.idempotency_key,
  };
  let st: JobState<ArchReview> = BigAnalysis().poll<ArchReview>(job);
  // Translate SERVER-authoritative status into UI state — the state machine a
  // blocking call never needs (background §"surfacing status in a UI").
  match (st.status) {
    JobStatus.Completed => ReviewStatus { state: "done",    review: st.value, detail: null },
    JobStatus.Failed    => ReviewStatus { state: "failed",  review: null,     detail: st.error },
    JobStatus.Cancelled => ReviewStatus { state: "cancelled", review: null,   detail: null },
    JobStatus.InProgress => ReviewStatus { state: "running", review: null,    detail: null },
    JobStatus.Queued     => ReviewStatus { state: "queued",  review: null,    detail: null },
  }
}

class ReviewStatus {
  state: string
  review: ArchReview?
  detail: string?
}


// ============================================================================
// (D) Blocking convenience: a CLI / worker that just wants the answer and is
//     willing to wait. wait_for() with backoff. One line, durable underneath.
//     (`wait_for`, not `await`: `await` is the BEP-034 keyword for in-process
//     Futures; this is a server-side poll loop.)
// ============================================================================

function ReviewRepoBlocking(repo: string) -> ArchReview
    throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
  // wait_for() generates its own idempotency key (single caller, no external retry
  // to reconcile with). BackoffCadence avoids hammering the retrieve endpoint.
  // The BackgroundError channel propagates: a server JobFailed / JobCancelled, a
  // transport hiccup wrapped as baml.UnknownError. The caller (G' below) triages.
  BigAnalysis().wait_for<ArchReview>(
    render_review_prompt(repo),
    BackoffCadence { base_ms: 1000, cap_ms: 30000 },
  )
}

// (D') The matching consumer catch — most-specific-first, the new error model.
// A caller that wants a degraded value instead of a propagated throw triages the
// BackgroundError channel: concrete server outcomes, then the interface (transport
// classifiers), then the UnknownError escape hatch.
function ReviewRepoOrPlaceholder(repo: string) -> ArchReview {
  ReviewRepoBlocking(repo) catch (e) {
    let f: JobFailed             => ArchReview { summary: "[job failed: " + f.reason, risks: [], score: 0 };
    let c: JobCancelled          => ArchReview { summary: "[job cancelled]", risks: [], score: 0 };
    let b: baml.errors.BackgroundError => {
      // Transport-classified (rate limit / network) vs everything else.
      if (b.is_rate_limit()) {
        ArchReview { summary: "[rate limited; retry later]", risks: [], score: 0 }
      } else {
        ArchReview { summary: "[background error]", risks: [], score: 0 }
      }
    }
    let u: baml.UnknownError     => ArchReview { summary: "[unknown error]", risks: u.message, score: 0 };
  }
}


// ============================================================================
// (E) Portability: the SAME flow against the DIVERGENT Anthropic batch provider.
//
//     AnthropicBatch's poll does a SECOND fetch of a JSONL results file on
//     completion and correlates the one line by custom_id — none of which appears
//     here. The app code is byte-for-byte (E) vs (D), proving the Job<T> handle
//     absorbs the inline-vs-batch divergence. The leak is real but contained.
// ============================================================================

function ReviewRepoBatch(repo: string) -> ArchReview
    throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
  BatchClaude().wait_for<ArchReview>(
    render_review_prompt(repo),
    BackoffCadence { base_ms: 5000, cap_ms: 60000 },   // batches are slower; poll lazily
  )
}


// ============================================================================
// (F) The Poller combinator: explicit cadence + deadline + caller-owned key, and
//     a clean "still running, come back later" path that re-drives the SAME job
//     instead of re-submitting. This is the reconciliation story end to end.
// ============================================================================

function ReviewWithDeadline(repo: string, request_id: string) -> ReviewStatus
    throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
  let poller = Poller {
    inner: OpenAIResponses.background("gpt-5"),
    cadence: BackoffCadence { base_ms: 2000, cap_ms: 20000 },
    max_attempts: 8,
  };
  // start() forwards the APP's key — a retry of this endpoint reuses the same
  // server job. drive() polls up to max_attempts, then yields control.
  let job: Job<ArchReview> = poller.start<ArchReview>(render_review_prompt(repo), request_id);
  save_job_handle(request_id, job.id, job.owner, job.idempotency_key);

  let st: JobState<ArchReview> = poller.drive<ArchReview>(job);
  if (st.terminal) {
    match (st.status) {
      JobStatus.Completed => ReviewStatus { state: "done", review: st.value, detail: null },
      _                   => ReviewStatus { state: "failed", review: null, detail: st.error },
    }
  } else {
    // Deadline reached, job alive. The handle is already persisted; a later
    // CheckReview / drive resumes it. No re-submit, no double job.
    ReviewStatus { state: "running", review: null, detail: "exceeded poll budget; resume by id" }
  }
}


// ============================================================================
// (G) The honest failure: PlainGpt() is store:false, not Background. The
//     generated .background companion `match`es for Background, misses, and
//     ERRORS — there is no fallback (a one-shot call cannot mint a durable,
//     resumable id). Contrast .stream, which CAN degrade to .call.
// ============================================================================

function TryBackgroundOnPlain(repo: string) -> Job<ArchReview>
    throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
  // At runtime this throws baml.errors.Unsupported: "client is not a background
  // provider". The app must choose a Background-capable client to get a Job.
  // NOTE: Unsupported is legal on this channel only because it implements
  // baml.errors.BackgroundError too — and BackgroundError is net-new (see eval (e)),
  // so the canonical Unsupported declaration had to be widened to add it.
  ReviewRepoPlain.background<ArchReview>(repo, "req-xyz")
    catch (e) {
      let un: baml.errors.Unsupported => baml.sys.panic("not a background provider: " + un.message);
      let u: baml.UnknownError        => baml.sys.panic("background companion failed");
    }
}

function ReviewRepoPlain(repo: string) -> ArchReview {
  client: PlainGpt()
  prompt #"Review {{ repo }}."#
}


// ---- host-side app sinks (the application provides these; extern here) ------
function save_job_handle(request_id: string, id: string, owner: string, idem: string) -> null { $rust_io_function }
function load_job_handle(request_id: string) -> StoredJob { $rust_io_function }
function render_review_prompt(repo: string) -> baml.llm.PromptAst { $rust_io_function }

class StoredJob {
  id: string
  owner: string
  idempotency_key: string
}
