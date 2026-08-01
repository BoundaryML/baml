// Idle-watermark suppression workload. A runner keeps this boundary alive for
// the requested wall duration without producing calling-context churn.
function IdleAgent(milliseconds: int) -> int {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(milliseconds));
  milliseconds
}
