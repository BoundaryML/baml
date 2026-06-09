export type LlmCompanionFunction = 'render_prompt' | 'build_request';

export function companionFunctionName(functionName: string, companion: LlmCompanionFunction): string {
  return `${functionName}$${companion}`;
}
