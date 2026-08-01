class TranscriptMessage {
  sequence int
  content string
}

// The harness supplies a 64 KiB or 1 MiB prompt and captures every returned
// prefix for N in {16, 32, 64, 128}.
function TranscriptAppend(prompt: string, messages: int) -> TranscriptMessage[][] {
  let transcript: TranscriptMessage[] = [];
  let prefixes: TranscriptMessage[][] = [];
  let sequence = 0;
  while (sequence < messages) {
    transcript += [TranscriptMessage { sequence, content prompt }];
    prefixes += [transcript];
    sequence += 1;
  }
  prefixes
}
