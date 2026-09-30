# Heartbeat v1 goldens

These are fixed wire examples for the proposed client/BCS contract, not evidence
of acceptance by a deployed BCS server. Credentials and `.invalid` URLs are
synthetic and grant no access.

- `sender.json` fixes the endpoint path, method, application headers, policy,
  empty successful response, and the RUNNING/DRAINING/DISABLED request bodies.
- `reordered-messages.json` is the arrival sequence 1, 3, 3, 2 for one producer:
  an exact duplicate followed by an older RUNNING message.
- `reordered-observations.json` fixes the expected highest sequence and state
  after every arrival. The late or duplicate messages must not regress state.

`golden_heartbeat.rs` exercises the real `Heartbeat::run` HTTP sender for each
state against the fixed response, then compares each received request with the
fixture. Only the local server origin and the random process session/monotonic
sequence are substituted. The test separately checks that the actual session is
a UUID shared across sender instances and that actual sequences increase.
Headers, method, endpoint path, query, policy and producer state are not inferred
from the request or rewritten to make it match.

The reordered-observation test is a reference server fold, not a BCS server
implementation test. BCS still owns organization authorization, authoritative
receipt timestamps, and stale-liveness policy. A heartbeat is not an upload
acknowledgement, proof of progress, or completion event.

JSON object order and whitespace are not protocol requirements. Golden JSON is
compared as a complete object; unknown liveness fields are rejected. Any future
wire change must deliberately update these files and be coordinated with BCS.
