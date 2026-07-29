enum ReplayKind {
    Safe,
    RequiresIdempotencyKey,
    Never,
}

class ReplayPolicy {
    kind: ReplayKind,
    idempotency_key: string?,
}
