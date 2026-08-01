function RecursiveFrame(depth: int) -> int {
  if (depth == 0) {
    return 1;
  }
  1 + RecursiveFrame(depth - 1)
}

// Below the fold threshold: must retain all 200 frames.
function depth_200() -> int { RecursiveFrame(200) }

// Above the fold threshold: must report RECURSION_FOLD and folded_frames.
function depth_1024() -> int { RecursiveFrame(1024) }
