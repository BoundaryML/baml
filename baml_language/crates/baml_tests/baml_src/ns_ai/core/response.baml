class Usage {
  input_tokens: int,
  output_tokens: int,
  cached_input_tokens: int?,
  reasoning_tokens: int?,
  cost_usd: float?,
}

class Meta {
  provider: string,
  model: string?,
  request_id: string?,
  finish_reason: string?,
  usage: Usage?,
  attributes: map<string, unknown>,
  raw: json?,
}

class Response<T> {
  value: T,
  meta: Meta,
}

function aggregate_usage(metas: Meta[]) -> Usage? throws never {
  let saw_usage = false;
  let input = 0;
  let output = 0;
  let cached = 0;
  let reasoning = 0;
  let cost = 0.0;
  for (let meta in metas) {
    match (meta.usage) {
      null => {},
      let usage: Usage => {
        saw_usage = true;
        input = input + usage.input_tokens;
        output = output + usage.output_tokens;
        cached = cached + (usage.cached_input_tokens ?? 0);
        reasoning = reasoning + (usage.reasoning_tokens ?? 0);
        cost = cost + (usage.cost_usd ?? 0.0);
      },
    }
  }
  if (!saw_usage) { return null; }
  Usage {
    input_tokens: input,
    output_tokens: output,
    cached_input_tokens: cached,
    reasoning_tokens: reasoning,
    cost_usd: cost,
  }
}
