// The production bugs the self-fixing versions find, one per lap. Each fix adds a line to the prompt and
// a test. The prompt below compiles with baml check; the failures are example data.
export const FIXES = [
  {
    failure: 'sarcastic tickets labelled "other"',
    rule: 'Read sarcasm as what the customer means.',
    test: 'sarcastic_ticket',
  },
  {
    failure: 'Spanish tickets labelled "other"',
    rule: 'Tickets can be in any language.',
    test: 'spanish_ticket',
  },
  {
    failure: 'angry refund requests labelled "bug"',
    rule: 'A refund request is billing, however angry.',
    test: 'angry_refund',
  },
];

/** The classify function with the given rules in its prompt, and where the prompt's text starts and ends. */
export function classifySource(rules: string[]) {
  const lines = [
    'function classify(text: string) -> Verdict {',
    '  client: "openai/gpt-6-luna"',
    '  prompt: `',
    '    Classify this support ticket.',
    ...rules.map((r) => `    ${r}`),
    '    ${text}',
    '  `',
    '}',
  ];
  const prose = Array.from({ length: rules.length + 3 }, (_, i) => 3 + i);
  return { lines, prose };
}

export const RED = '#E5484D';
// Marks the code a query or a step is about, bright enough to find at a glance.
export const HIGHLIGHT = '#FFD43B';
