// Middle frame: calls ChainInner (same `chain` namespace, different file).
// Has no `throws` clause — the contract is inferred through the call.

/// Middle step — calls ChainInner.
function ChainMiddle() -> string {
  ChainInner()
}
