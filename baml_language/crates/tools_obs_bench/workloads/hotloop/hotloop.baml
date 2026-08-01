// CPU-only reproduction of the legacy one-event-per-call growth bug.
function HotLeaf(value: int) -> int {
  value + 1
}

function HotLoop(calls: int) -> int {
  let checksum = 0;
  let index = 0;
  while (index < calls) {
    checksum += HotLeaf(index);
    index += 1;
  }
  checksum
}

function bench_500k() -> int { HotLoop(500000) }
function bench_5m() -> int { HotLoop(5000000) }
function bench_15m() -> int { HotLoop(15000000) }
function bench_36m() -> int { HotLoop(36000000) }
