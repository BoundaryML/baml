// Fixed work-shape entry point. The harness varies `calls` by 100x while
// holding the wall-time window constant.
function RateLeaf(work: int) -> int {
  let value = 0;
  let index = 0;
  while (index < work) {
    value += index % 17;
    index += 1;
  }
  value
}

function bench_rate(calls: int, work: int) -> int {
  let checksum = 0;
  let index = 0;
  while (index < calls) {
    checksum += RateLeaf(work);
    index += 1;
  }
  checksum
}
