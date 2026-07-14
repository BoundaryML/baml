enum ReplayKind {
  Never
  Safe
  Idempotent
}

class ReplayPolicy {
  kind: ReplayKind,
  idempotency_key: string?,
}

function can_replay(policy: ReplayPolicy) -> bool throws never {
  policy.kind == ReplayKind.Safe || policy.kind == ReplayKind.Idempotent
}

