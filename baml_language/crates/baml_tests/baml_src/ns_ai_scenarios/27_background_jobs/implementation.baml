// 27 — background work returns a Job<T> resource. The serializable boundary is
// JobToken; the resource itself keeps the configured provider and parser attached.

class s27_ScriptedJob<T> {
  id: string,
  owner: string,
  result: T,
  remaining_polls: int,
  cancelled: bool,

  implements root.ai.Job<T> {
    function poll(self) -> root.ai.Done<T> | root.ai.Pending | root.ai.Failed | root.ai.Cancelled
        throws baml.errors.UnknownError {
      if (self.cancelled) { return root.ai.Cancelled {}; }
      if (self.remaining_polls > 0) {
        self.remaining_polls = self.remaining_polls - 1;
        return root.ai.Pending { retry_after: null };
      }
      root.ai.Done<T> {
        value: self.result,
        meta: root.ai.Meta {
          provider: self.owner, model: null, request_id: self.id,
          finish_reason: "completed", usage: null, attributes: {}, raw: null,
        },
      }
    }

    function cancel(self) -> null throws baml.errors.UnknownError {
      self.cancelled = true;
      null
    }
    function token(self) -> root.ai.JobToken throws never {
      root.ai.JobToken { provider: self.owner, id: self.id }
    }
    function cleanup(self) -> null throws never { null }
  }
}

class s27_ScriptedJobs {
  owner: string,
  remaining_polls: int,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { self.owner }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }

  implements root.ai.Background {
    function submit<T>(
      self,
      request: root.ai.Request<T>,
      options: root.ai.BackgroundOptions,
    ) -> root.ai.Job<T> throws baml.errors.UnknownError {
      let key = options.idempotency_key ?? "unkeyed";
      let id = "job-" + key;
      let result = root.ai.fake_value<T, string>("done " + id);
      s27_ScriptedJob<T> {
        id: id, owner: self.owner, result: result,
        remaining_polls: self.remaining_polls, cancelled: false,
      }
    }

    function resume_job<T>(self, token: root.ai.JobToken) -> root.ai.Job<T>
        throws baml.errors.UnknownError {
      if (token.provider != self.owner) {
        throw baml.errors.UnknownError { data: token, message: ["job token belongs to a different provider"] };
      }
      let result = root.ai.fake_value<T, string>("done " + token.id);
      s27_ScriptedJob<T> {
        id: token.id, owner: self.owner, result: result,
        remaining_polls: 0, cancelled: false,
      }
    }
  }
}

function s27_poll_label(job: root.ai.Job<string>) -> string throws baml.errors.UnknownError {
  match (job.poll()) {
    let pending: root.ai.Pending => "pending",
    let done: root.ai.Done<string> => done.value,
    let failed: root.ai.Failed => "failed",
    let cancelled: root.ai.Cancelled => "cancelled",
  }
}
