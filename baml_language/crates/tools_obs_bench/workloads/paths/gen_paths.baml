// Small checked-in seed for the path-cardinality workload. Release and nightly
// runs replace this shape with `obs-bench gen-paths --paths P`, where
// P is selected from workloads/paths/gen_paths.toml.
function SeedLeaf0(value: int) -> int { value }
function SeedLeaf1(value: int) -> int { value + 1 }
function SeedLeaf2(value: int) -> int { value + 2 }
function SeedLeaf3(value: int) -> int { value + 3 }

function SeedPath0(value: int) -> int { SeedLeaf0(value) }
function SeedPath1(value: int) -> int { SeedLeaf1(value) }
function SeedPath2(value: int) -> int { SeedLeaf2(value) }
function SeedPath3(value: int) -> int { SeedLeaf3(value) }

function GenPathsSeed(rounds: int) -> int {
  let checksum = 0;
  let round = 0;
  while (round < rounds) {
    checksum += SeedPath0(round);
    checksum += SeedPath1(round);
    checksum += SeedPath2(round);
    checksum += SeedPath3(round);
    round += 1;
  }
  checksum
}
