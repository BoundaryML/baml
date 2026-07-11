class Usage {
  input_tokens: int,
  output_tokens: int,
  cached_input_tokens: int?,
  reasoning_tokens: int?,
  cost_usd: float?,

  function estimated_cost(self, price: TokenPrice) -> float throws never {
    let input = float.parse(self.input_tokens.to_string()) catch (e) { _ => 0.0 };
    let output = float.parse(self.output_tokens.to_string()) catch (e) { _ => 0.0 };
    (input * price.input_per_million + output * price.output_per_million) / 1000000.0
  }
}

class TokenPrice {
  input_per_million: float,
  output_per_million: float,
}

class Meta {
  provider: string,
  model: string?,
  request_id: string?,
  finish_reason: string?,
  usage: Usage?,
  attributes: map<string, unknown>,
  // `json` already includes null; spelling this `json?` creates a nested
  // recursive union that is invalid for SAP's global type database.
  raw: json,
  blocks: ModelBlock[]?,
  citations: Citation[]?,
  logprobs: Logprob[]?,

  function reasoning_blocks(self) -> ReasoningBlock[] throws never {
    let reasoning: ReasoningBlock[] = [];
    for (let block in self.blocks ?? []) {
      match (block) {
        let value: ReasoningBlock => reasoning.push(value),
        _ => {},
      }
    }
    reasoning
  }
}

class Citation {
  url: string,
  title: string?,
}

class Logprob {
  token: string,
  logprob: float,
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
