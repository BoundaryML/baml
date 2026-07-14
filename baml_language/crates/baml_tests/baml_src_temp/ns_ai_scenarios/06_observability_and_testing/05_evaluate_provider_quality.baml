function resolution_score(value: Resolution) -> float throws never {
  let score = 0.0;
  if (value.category == "billing") { score = score + 0.5; }
  if (value.reply.length() > 10) { score = score + 0.5; }
  score
}

test "guide 06.05: provider evaluation is ordinary typed application code" {
  let value = ResolveTicket_manual(sample_ticket(), fake_resolution());
  assert.equal(resolution_score(value), 1.0)
}

