import type { CodeAnnotation } from './annotation-svg';

export const annotationSpecs = {
  'baml-define': [
    {
      kind: 'syntax',
      label: 'Compiler tracks this error',
      mark: 'underbrace',
      text: 'throw BadToolInput',
    },
    {
      kind: 'success',
      label: 'Return the value',
      mark: 'underbrace',
      text: '    tool\n',
    },
  ],
  'baml-propagate': [
    {
      kind: 'success',
      label: 'Use the returned value',
      mark: 'underbrace',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: Match the literal source expression, including interpolation syntax.
      text: '`Tool: ${check_tool_name(tool)}`',
    },
  ],
  'baml-recover': [
    {
      kind: 'recovery',
      label: 'Check that no error escapes',
      mark: 'underbrace',
      text: 'throws never',
    },
    {
      kind: 'success',
      label: 'Same successful expression',
      mark: 'underbrace',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: Match the literal source expression, including interpolation syntax.
      text: '`Tool: ${check_tool_name(tool)}`',
    },
    {
      kind: 'recovery',
      label: 'Return a fallback',
      mark: 'bracket',
      text: 'catch (e) {\n        BadToolInput => e.message,\n    }',
    },
  ],
  'effect-define': [
    {
      kind: 'syntax',
      label: 'Return type includes errors',
      mark: 'underbrace',
      text: 'Effect.Effect<string, BadToolInput, never>',
    },
    {
      kind: 'syntax',
      label: 'Wrap the successful value',
      mark: 'underbrace',
      text: 'Effect.succeed',
    },
  ],
  'effect-propagate': [
    {
      kind: 'syntax',
      label: 'Run these steps in an Effect',
      mark: 'underbrace',
      text: 'Effect.gen(function* () {',
    },
    {
      kind: 'syntax',
      label: 'Read the value; propagate errors',
      mark: 'circle',
      text: 'yield*',
    },
    {
      kind: 'success',
      label: 'Use the returned value',
      mark: 'underbrace',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: Match the literal source expression, including interpolation syntax.
      text: '`Tool: ${name}`',
    },
  ],
  'effect-recover': [
    {
      kind: 'recovery',
      label: 'No typed errors remain',
      mark: 'underbrace',
      occurrence: 1,
      text: 'never',
    },
    {
      kind: 'success',
      label: 'Same successful expression',
      mark: 'underbrace',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: Match the literal source expression, including interpolation syntax.
      text: '`Tool: ${name}`',
    },
    {
      kind: 'recovery',
      label: 'Return a fallback',
      mark: 'bracket',
      text: '.pipe(\n    Effect.catchAll((error) => Effect.succeed(error.message)),\n  )',
    },
  ],
  'rust-define': [
    {
      kind: 'syntax',
      label: 'Return type includes errors',
      mark: 'underbrace',
      text: 'Result<String, BadToolInput>',
    },
    {
      kind: 'syntax',
      label: 'Wrap the successful value',
      mark: 'underbrace',
      text: 'Ok(',
    },
    {
      kind: 'success',
      label: 'Successful value',
      mark: 'underbrace',
      occurrence: 2,
      text: 'tool.to_string()',
    },
  ],
  'rust-propagate': [
    {
      kind: 'syntax',
      label: 'Wrap the successful value',
      mark: 'underbrace',
      text: 'Ok(',
    },
    {
      kind: 'success',
      label: 'Call the function',
      mark: 'underbrace',
      text: 'check_tool_name(tool)',
    },
    {
      kind: 'syntax',
      label: 'Propagate the error',
      mark: 'circle',
      text: '?',
    },
  ],
  'rust-recover': [
    {
      kind: 'success',
      label: 'Use the returned value',
      mark: 'underbrace',
      text: 'format!("Tool: {}", tool)',
    },
    {
      kind: 'recovery',
      label: 'Return a fallback',
      mark: 'bracket',
      text: 'Err(error) => error.message,',
    },
  ],
  'typescript-define': [
    {
      kind: 'syntax',
      label: 'Thrown errors aren\u2019t recorded',
      mark: 'underbrace',
      text: '): string',
    },
    {
      kind: 'success',
      label: 'Return the value',
      mark: 'underbrace',
      text: 'return tool',
    },
  ],
  'typescript-propagate': [
    {
      kind: 'success',
      label: 'Use the returned value',
      mark: 'underbrace',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: Match the literal source expression, including interpolation syntax.
      text: '`Tool: ${checkToolName(tool)}`',
    },
  ],
  'typescript-recover': [
    {
      kind: 'recovery',
      label: 'Wrap the call in try',
      mark: 'bracket',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: Match the literal source expression, including interpolation syntax.
      text: 'try {\n    return `Tool: ${checkToolName(tool)}`',
    },
    {
      kind: 'success',
      label: 'Same successful expression',
      mark: 'underbrace',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: Match the literal source expression, including interpolation syntax.
      text: '`Tool: ${checkToolName(tool)}`',
    },
    {
      kind: 'recovery',
      label: 'Check the error in catch',
      mark: 'bracket',
      text: '} catch (error: unknown) {\n    if (error instanceof BadToolInput) {\n      return error.message\n    }\n    throw error\n  }',
    },
  ],
} satisfies Record<string, CodeAnnotation[]>;

export type CodeAnnotationId = keyof typeof annotationSpecs;
