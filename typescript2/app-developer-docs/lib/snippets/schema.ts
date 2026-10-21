import { z } from 'zod';

export const expectedDiagnosticSchema = z
  .object({
    code: z.string().regex(/^E\d{4}$/),
    messageContains: z.string().min(1).optional(),
  })
  .strict();

const successExpectationSchema = z
  .object({
    status: z.literal('success'),
  })
  .strict();

const failureExpectationSchema = z
  .object({
    diagnostics: z.array(expectedDiagnosticSchema).min(1).optional(),
    status: z.literal('failure'),
  })
  .strict();

export const snippetExpectationSchema = z.discriminatedUnion('status', [
  successExpectationSchema,
  failureExpectationSchema,
]);

export const snippetMetadataSchema = z
  .object({
    expect: snippetExpectationSchema,
  })
  .strict();

export type ExpectedDiagnostic = z.output<typeof expectedDiagnosticSchema>;
export type SnippetExpectation = z.output<typeof snippetExpectationSchema>;
export type SnippetMetadata = z.output<typeof snippetMetadataSchema>;

export const successfulSnippetExpectation: SnippetExpectation = {
  status: 'success',
};
