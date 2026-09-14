export const highlightLabels = {
  recovery: 'Recovery',
  success: 'Successful expression',
  syntax: 'Required syntax',
} as const;

export type CodeHighlight = {
  kind: keyof typeof highlightLabels;
  text: string;
  occurrence?: number;
};

export function resolveCodeHighlights(
  code: string,
  highlights: CodeHighlight[],
) {
  const ranges = highlights
    .map(({ kind, text, occurrence: selectedOccurrence }) => {
      const occurrence = selectedOccurrence ?? 1;
      if (!text || !Number.isInteger(occurrence) || occurrence < 1) {
        throw new Error(
          'Code highlights require text and a positive occurrence.',
        );
      }
      let start = -1;
      for (let i = 0; i < occurrence; i++) {
        start = code.indexOf(text, start + 1);
        if (start === -1) {
          throw new Error(`Code highlight not found: ${JSON.stringify(text)}`);
        }
      }
      if (
        selectedOccurrence === undefined &&
        code.indexOf(text, start + 1) !== -1
      ) {
        throw new Error(
          `Ambiguous code highlight; specify occurrence: ${JSON.stringify(text)}`,
        );
      }
      return { end: start + text.length, kind, start };
    })
    .sort((a, b) => a.start - b.start);
  for (let i = 1; i < ranges.length; i++) {
    if (ranges[i].start < ranges[i - 1].end) {
      throw new Error('Code highlights must not overlap.');
    }
  }
  return ranges;
}
