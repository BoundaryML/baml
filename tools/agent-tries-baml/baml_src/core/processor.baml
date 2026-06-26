// The claim loop, modeled as a BAML interface whose DEFAULT methods carry the
// whole machinery (SSE wake -> drain, poll backstop, per-claim heartbeat,
// claim -> process -> fail-on-same-field). Ports libs/bench_core/processor.py.
//
// A concrete processor is a small class that `implements Processor`, supplying
// four accessors (config/service/worker_id) + the stage work (process). It
// inherits run()/drain()/run_one()/heartbeat/presence/backstop unchanged.
//
// The claimed document is passed to `process` as raw JSON text; each processor
// decodes it to its own concrete type (baml.json.from_string<T>), so the
// interface needs no associated `Item` type. `_id` is pulled out generically via
// lenient decode (DocId).
//
// CONCURRENCY SAFETY: a detached spawn that throws surfaces an uncatchable host
// error at the parent's next await (BEP-034). Every background loop here
// (heartbeat/presence/backstop) therefore swallows ALL errors internally and
// never settles — exactly as the Python loops do.

class ProcessorConfig {
    role: string,
    table: string,
    claim_value: string,
    claim_into: string,
    claim_field: string,
    claim_index: string,
    lease_ms: int,
    heartbeat_ms: int,
    poll_backstop_ms: int,
    presence_ms: int,
    batch: bool,
}

// Raised by `process` to signal the item should be failed. Processors wrap their
// internal errors (ServiceError/Io/Timeout/LLM) into this.
class ProcessError {
    message: string,
}

// Lenient shape to pull `_id` off any claimed doc.
class DocId {
    _id: string?,
}

// Patch body for recording a failure reason.
class LastErrorPatch {
    lastError: string,
}

interface Processor {
  // ---- abstract surface each processor supplies ----
  function config(self) -> ProcessorConfig
  function service(self) -> ServiceClient
  function worker_id(self) -> string
  // Handle one claimed item (raw JSON). On success the processor performs its
  // own transition to the next status; on failure it throws ProcessError and the
  // base marks the item failed.
  function process(self, raw: string) -> int throws ProcessError

  // ================= default methods (the shared loop) =================

  // Claim and run items until the queue is empty (or just one in batch mode).
  function drain(self) -> int throws ServiceError | baml.errors.Io | baml.errors.Timeout {
    let cfg = self.config();
    let n = 0;
    while (true) {
      let claimed = self.service().claim(cfg.table, self.worker_id(), cfg.lease_ms, cfg.claim_value, cfg.claim_into, cfg.claim_field, cfg.claim_index);
      match (claimed) {
        null => {
          break;
        },
        let raw: string => {
          self.run_one(raw);
          n = n + 1;
          if (cfg.batch) {
            break;
          }
        },
      };
    }
    return n
  }

  // Run process() for one item under a heartbeat, failing it (on the SAME field
  // it was claimed on) if process throws. Never throws — drain stays alive.
  function run_one(self, raw: string) -> int {
    let cfg = self.config();
    let id = self.doc_id(raw);
    let p: Processor = self;
    let hb = spawn { p.heartbeat_loop(id) };
    self.set_presence("busy", id);
    let result = self.process(raw) catch (e) {
      _ => {
        self.fail_item(id, baml.unstable.string(e));
        0
      }
    };
    let _ = hb.cancel();
    self.set_presence("idle", "");
    return result
  }

  // Best-effort: record lastError + transition to "failed" on the claim field.
  function fail_item(self, id: string, message: string) -> int {
    let cfg = self.config();
    let patch = baml.json.to_string(LastErrorPatch { lastError: truncate(message, 2000) }) catch (_) {
      _ => "{}"
    };
    let _ = self.service().update(cfg.table, id, patch) catch (_) {
      _ => ""
    };
    let _ = self.service().transition(cfg.table, id, "failed", cfg.claim_field, "{}", true) catch (_) {
      _ => ""
    };
    return 0
  }

  // Pull `_id` off the claimed doc; "" when absent (best-effort).
  function doc_id(self, raw: string) -> string {
    let parsed = baml.json.from_string<DocId>(raw) catch (_) {
      _ => DocId { _id: null }
    };
    return match (parsed._id) {
      let v: string => v,
      null => "",
    }
  }

  // Renew the item's lease until cancelled. Swallows all errors (a transient
  // heartbeat failure must not kill the task, or the reaper steals the item).
  function heartbeat_loop(self, id: string) -> int {
    let cfg = self.config();
    while (true) {
      let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(cfg.heartbeat_ms)) catch (_) {
        _ => null
      };
      let _ = self.service().heartbeat(cfg.table, id, cfg.lease_ms) catch (_) {
        _ => 0
      };
    }
    return 0
  }

  // Mirror presence to the workers roster — best-effort, never throws.
  function set_presence(self, status: string, id: string) -> int {
    let cfg = self.config();
    let item = if (id == "") { null } else { id };
    let _ = self.service().worker_heartbeat(self.worker_id(), cfg.role, status, item) catch (_) {
      _ => 0
    };
    return 0
  }

  // Re-assert presence periodically so the roster survives sweeps. Infinite,
  // swallows all errors.
  function presence_beat(self) -> int {
    let cfg = self.config();
    while (true) {
      let _ = self.service().worker_heartbeat(self.worker_id(), cfg.role, "idle", null) catch (_) {
        _ => 0
      };
      let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(cfg.presence_ms)) catch (_) {
        _ => null
      };
    }
    return 0
  }

  // Periodically drain to backstop dropped SSE wake-ups. Infinite, swallows all.
  function poll_backstop(self) -> int {
    let cfg = self.config();
    while (true) {
      let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(cfg.poll_backstop_ms)) catch (_) {
        _ => null
      };
      let _ = self.drain() catch (_) {
        _ => 0
      };
    }
    return 0
  }

  // One SSE connection: drain on every wake event until the stream closes.
  function sse_loop(self) -> int throws ServiceError | baml.errors.Io | baml.errors.Timeout {
    let cfg = self.config();
    let stream = self.service().events(cfg.table, cfg.claim_value, cfg.claim_field, cfg.claim_index);
    while (true) {
      match (stream.next()) {
        null => {
          break;
        },
        let line: string => {
          let _ = self.drain();
        },
      };
    }
    return 0
  }

  // The main loop: start backstop + presence, catch up on startup, then drain on
  // every SSE wake-up, reconnecting (with the backstop covering gaps) on drops.
  function run(self) -> int {
    let p: Processor = self;
    let _backstop = spawn { p.poll_backstop() };
    let _presence = spawn { p.presence_beat() };
    let _ = self.drain() catch (_) {
      _ => 0
    };
    while (true) {
      let _ = self.sse_loop() catch (_) {
        _ => {
          let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(5000)) catch (_) {
            _ => null
          };
          0
        }
      };
    }
    return 0
  }
}

function truncate(s: string, max: int) -> string {
    if (s.length() <= max) {
        return s;
    }
    return s.substring(0, max);
}
