// utils.ts
import { type TestResult } from './types';

export function parseJSONL<T>(jsonl: string): T[] {
  // eslint-disable-next-line @typescript-eslint/no-unsafe-return
  return (
    jsonl
      .trim()
      .split('\n')
      // eslint-disable-next-line @typescript-eslint/no-unsafe-return
      .map((line) => JSON.parse(line))
  );
}

const MODEL_COSTS_PER_1M_TOKENS = {
  'gpt-3.5-turbo-0125': [0.5, 1.5],
  'gpt-4o-2024-05-13': [5, 15],
  'gpt-4o-2024-08-06': [2.5, 10],
  'gpt-4o-mini-2024-07-18': [0.15, 0.6],
  'claude-3-haiku-20240307': [0.25, 1.25],
  'claude-3-5-sonnet-20240620': [3, 15],
  'ollama-llama3.1': [0, 0],
};

export function calculateCost(
  input_tokens: number,
  output_tokens: number,
  model: string,
): number {
  console.log(input_tokens, output_tokens, model);
  return (
    (input_tokens *
      MODEL_COSTS_PER_1M_TOKENS[
        model as keyof typeof MODEL_COSTS_PER_1M_TOKENS
      ][0]! +
      output_tokens *
        MODEL_COSTS_PER_1M_TOKENS[
          model as keyof typeof MODEL_COSTS_PER_1M_TOKENS
        ][1]!) /
    1e6
  );
}

export function calculateAccuracy(
  results: TestResult[],
  model: string,
  test_type: string | null,
  qualifier: string,
): {
  accuracy: number;
  latency: { avg: number; std: number };
  tps: { avg: number; std: number };
  input_tokens: number;
  output_tokens: number;
  cost: number;
} {
  let valid = 0;
  let total = 0;
  const latency: number[] = [];
  const tps: number[] = [];
  let input_tokens = 0;
  let output_tokens = 0;

  results.forEach((result) => {
    if (
      result.model === model &&
      (test_type === null || result.test_type === test_type) &&
      result.qualifier === qualifier
    ) {
      total += 1;
      if (result.valid) {
        valid += 1;
      }
      latency.push(result.latency);
      tps.push(
        result.latency === 0 ? 0 : result.output_token_count / result.latency,
      );
      input_tokens += result.input_token_count;
      output_tokens += result.output_token_count;
    }
  });

  return {
    accuracy: total === 0 ? 0 : (valid / total) * 100,
    input_tokens: input_tokens,
    output_tokens: output_tokens,
    cost: calculateCost(input_tokens, output_tokens, model),
    tps: {
      avg: tps.length === 0 ? 0 : tps.reduce((a, b) => a + b) / tps.length,
      std:
        tps.length === 0
          ? 0
          : Math.sqrt(
              tps.reduce(
                (a, b) =>
                  a + Math.pow(b - tps.reduce((a, b) => a + b) / tps.length, 2),
                0,
              ) / tps.length,
            ),
    },
    latency: {
      avg:
        latency.length === 0
          ? 0
          : latency.reduce((a, b) => a + b) / latency.length,
      std:
        latency.length === 0
          ? 0
          : Math.sqrt(
              latency.reduce(
                (a, b) =>
                  a +
                  Math.pow(
                    b - latency.reduce((a, b) => a + b) / latency.length,
                    2,
                  ),
                0,
              ) / latency.length,
            ),
    },
  };
}

export const calculateContent = (
  results: TestResult[],
  selectedModel: string,
  selectedTestType: string | null,
) => {
  const content: {
    data: {
      bfcl: ReturnType<typeof calculateAccuracy>;
      baml: ReturnType<typeof calculateAccuracy>;
      FC?: ReturnType<typeof calculateAccuracy>;
      'FC-strict'?: ReturnType<typeof calculateAccuracy>;
    };
    best: {
      accuracy: number;
      latency: { avg: number };
      cost: number;
      tps: { avg: number };
    };
  } = {
    data: {
      bfcl: calculateAccuracy(results, selectedModel, selectedTestType, 'bfcl'),
      baml: calculateAccuracy(results, selectedModel, selectedTestType, 'baml'),
    },
    best: {
      accuracy: 0,
      latency: { avg: 0 },
      cost: 0,
      tps: { avg: 0 },
    },
  };

  const fc = calculateAccuracy(results, selectedModel, selectedTestType, 'FC');
  if (fc.cost > 0) {
    content.data.FC = fc;
  }
  const fc_strict = calculateAccuracy(
    results,
    selectedModel,
    selectedTestType,
    'FC-strict',
  );
  if (fc_strict.cost > 0) {
    content.data['FC-strict'] = fc_strict;
  }

  content.best.accuracy = Object.values(content.data).reduce((a, b) =>
    a.accuracy > b.accuracy ? a : b,
  ).accuracy;
  content.best.latency.avg = Object.values(content.data).reduce((a, b) =>
    a.latency.avg < b.latency.avg ? a : b,
  ).latency.avg;
  content.best.cost = Object.values(content.data).reduce((a, b) =>
    a.cost < b.cost ? a : b,
  ).cost;
  content.best.tps.avg = Object.values(content.data).reduce((a, b) =>
    a.tps.avg > b.tps.avg ? a : b,
  ).tps.avg;
  return content;
};
