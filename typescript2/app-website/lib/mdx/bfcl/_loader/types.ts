// types.ts
export interface TestResult {
  result: Array<{ [key: string]: string | any }> | string | null;
  model_result_decoded: Array<{ [key: string]: string | any }> | null;
  input_token_count: number;
  output_token_count: number;
  latency: number;
  prompt: {
    messages: Array<
      { role: string } & (
        | { content: string }
        | { parts: Array<{ Text: string }> }
      )
    >;
    system?: string;
    parsed?: string;
    tools?: Array<Record<string, any>>;
  };
  id: string;
  valid: boolean;
  error:
    | null
    | [string]
    | [string, ...Array<{ [key: string]: { sub_error: string[] } }>];
  error_type: null | string;
  ground_truth: Record<string, Record<string, any[]>>;
  qualifier: 'FC' | 'baml' | 'bfcl' | 'FC-strict';
  model: string;
  test_type: string;
  baml_error: null | string;
}

export type BCFLFunction = {
  name: string;
  description: string;
  parameters?: {
    properties: Record<
      string,
      {
        type: string;
        [key: string]: any;
      }
    >;
    [key: string]: any;
  };
};

export interface Question {
  idx: number;
  test_type: string;
  question: string;
  function: BCFLFunction | BCFLFunction[];
}
