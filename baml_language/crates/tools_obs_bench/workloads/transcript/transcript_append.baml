// obs-bench workload: transcript append (design §10.2 `transcript/transcript_append`).
//
// The value-plane quadratic: an agent loop re-submits its whole transcript
// every turn, so captured inputs grow N(N+1)/2 while distinct information
// grows ~N. C5 gates the content-addressed store on this file: growth
// exponent <= 1.2 and >= 20x byte reduction at N=64 with a 64 KiB prompt
// versus the legacy whole-body capture.
//
// `turn` is the captured seam: its input is the entire transcript so far
// (system prompt + all prior turns), exactly like an LLM chat call. The
// harness sizes `prompt_kib` at 64 (default legs) and 1024 (1 MiB legs) and
// sweeps N in {16..128}. Deterministic content: repeated 64-byte lines, new
// text per turn keyed on the turn index.

// One deterministic ~1 KiB paragraph, distinct per (seed).
function para(seed: int) -> string {
    let line = "turn-" + seed.to_string() + " lorem ipsum observability baml cct value dedupe xx\n";
    let s = "";
    let i = 0;
    while (i < 16) {
        s += line;
        i += 1;
    }
    s
}

// The captured "LLM" turn: takes the whole transcript, returns the new
// message to append. Body cost is trivial; capture cost is the point.
function turn(transcript: string, idx: int) -> string {
    para(idx)
}

function main(turns: int, prompt_kib: int) -> int {
    // System prompt: prompt_kib KiB of identical-per-run content (stored once
    // under content addressing, re-captured every turn under legacy).
    let sys_prompt = "";
    let k = 0;
    while (k < prompt_kib) {
        sys_prompt += para(-1);
        k += 1;
    }

    let transcript = sys_prompt;
    let t = 0;
    while (t < turns) {
        let reply = turn(transcript, t);
        transcript += reply;
        t += 1;
    }
    transcript.length()
}
