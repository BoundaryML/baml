# Durability (design notes)

This page is a design sketch, not a user guide. It lays out the options
for what durability BAML sessions should guarantee, what each option
costs, and the failure modes to watch. The journal format is designed so
we can start at tier 1 and add tiers without breaking existing journals.

## The tiers

**Tier 1 — snapshot/resume (baseline, in scope for v1).**
The journal is the state; `snapshot()`/`resume` and journal stores are
the whole story. A crash mid-turn loses the uncommitted turn; the caller
retries. No settlement, no receipts.

- Cost: near zero. Falls out of the journal design.
- Good enough for: request/response servers, CLIs, anything where the
  caller can retry a turn.
- Not enough for: fire-and-forget submissions, long unattended runs,
  background jobs.

**Tier 2 — admitted work settles (target for named instances and jobs).**
Once a submission is durably admitted, it reaches exactly one terminal
outcome — `completed`, `failed`, or `aborted` — through crashes and
restarts. Requires: admission records written before model work, receipts
(`send` returns one; `read(receipt)` re-attaches), a recovery pass that
classifies interrupted work from journal evidence, retry budgets and
timeouts per function.

- Cost: a recovery state machine, store transactions, a supervisor for
  running attempts. This is most of the implementation work in this BEP.
- Semantics: at-least-once execution, exactly-once recording. Committed
  work (recorded responses, tool results, steps) is never re-run;
  uncommitted work re-runs on the next attempt.

**Tier 3 — full deterministic replay (explicitly out of scope).**
Re-execute arbitrary agent code from the journal with all effects
short-circuited, Temporal-style. Requires banning or journaling every
nondeterminism source (time, random, iteration order) in any code a
session touches. The constraint surface is large and user-hostile; we get
most of the value from tier 2 plus pure policies. Revisit only with
evidence.

## Implementation options for tier 2

**Write-ahead batches.** The runner accumulates a turn's events and
commits them as one batch through `JournalStore.append`. Options: (a)
require stores to be transactional per batch (simple contract, pushes
work onto store authors); (b) frame batches in the entry format itself
(`batch_id` + commit marker) so recovery can discard unterminated batches
on any store. Lean: (b) — it keeps the store interface two functions and
makes half-written batches detectable everywhere, including files.

**Admission and settlement records.** Model submissions as events:
`Submitted { receipt, message }` before work, `Settled { receipt, outcome }`
after. Recovery scans for `Submitted` without `Settled` and classifies:
never started → re-run; response recorded but not settled → settle
completed; partial tool batch → re-run unfinished tools only; abort
intent recorded → settle aborted.

**Steps.** `baml.session.step(name, fn)` is journaled memoization:
completed steps append `StepCompleted { call_id, step, result_json }` and
are read back on re-runs of the same tool call. Cheap to implement,
disproportionate value — it is the difference between "retry re-charges
the customer" and "retry resumes at step 3".

**Instance locking.** Named instances need one live writer per ID.
Options: store-level lease (row lock / lease table), or a single-writer
process model per instance. The store-lease is the portable choice; the
interface grows one optional method (`lease/renew/release`).

## Things to look out for

- **Side effects re-run.** At-least-once execution means an uncommitted
  tool call executes again. This is inherent: the journal records that
  work ran, never the effect itself. Guidance we must document loudly:
  effectful tools are idempotent, keyed on `baml.session.call_id()`;
  multi-effect tools use steps; never guard effects with policy state
  (it is a cache, not a record).
- **Snapshot versioning.** Journals outlive code. Renamed tools, changed
  event schemas, and edited prompts all meet old journals. Needs: a
  format version in `SessionStarted`, additive-only evolution rules for
  built-in events, and a defined behavior when a resumed journal
  references a tool that no longer exists (surface to the model vs.
  fail).
- **Policy state rebuild.** On resume, `SessionState` is rebuilt by
  re-folding the journal through `update`. A policy whose state depends
  on anything outside the journal (clock, external reads) diverges after
  resume. Lintable in principle; document it as a hard rule regardless.
- **Partial streams.** A crash mid-stream leaves a half-received
  response. Rule: streamed output is not part of the journal until the
  final message commits with its batch; recovery treats a partial as
  never-happened and may inform the model on retry.
- **Compaction vs. recovery.** Recovery classifies from journal evidence;
  compaction must therefore never remove evidence, only add rendering
  instructions. Already guaranteed by the append-only rule; keep it that
  way when tempted to "garbage collect."
- **Duplicate submissions.** Crashed callers re-send. Receipts make
  re-reads safe; for creation, `new = true` (create-only) rejects
  duplicates. Servers in front of `baml serve` should pass through
  client-supplied receipts rather than minting new ones.
- **Cancellation races.** An interrupt racing a committing batch must
  produce one of two journals: batch committed then `Interrupted`, or no
  batch and `Interrupted`. Never an interleaving.
