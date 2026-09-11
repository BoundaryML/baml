const escapeXml = (value: string) =>
  String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
const PALETTES = {
  dark: {
    background: '#151515',
    border: '#303030',
    muted: '#ababab',
    recovery: '#bd9cf5',
    success: '#91d6b1',
    syntax: '#e6bd7c',
    text: '#e5e5e5',
  },
  light: {
    background: '#fbfbfa',
    border: '#ddddda',
    muted: '#61615f',
    recovery: '#7653b5',
    success: '#237849',
    syntax: '#94620c',
    text: '#242424',
  },
};
export type CodeAnnotation = {
  text: string;
  label: string;
  kind: 'success' | 'syntax' | 'recovery';
  occurrence?: number;
  mark?: 'underbrace' | 'circle' | 'bracket';
};

type AnnotationToken = {
  content: string;
  color?: string;
};

type AnnotatedSnippetInput = {
  code: string;
  tokens?: AnnotationToken[][];
  annotations: CodeAnnotation[];
  title?: string;
  theme?: 'light' | 'dark';
  fontFamily?: string;
  charWidth?: number;
};

type AnnotatedSnippetSvg = {
  svg: string;
  width: number;
  height: number;
};

/** Render source text and annotation shapes as a standalone SVG. */
export function renderAnnotatedSnippet({
  code,
  tokens,
  annotations,
  title = '',
  theme = 'dark',
  fontFamily = "'DejaVu Sans Mono','SFMono-Regular',Consolas,monospace",
  charWidth = 12.05,
}: AnnotatedSnippetInput): AnnotatedSnippetSvg {
  const palette = PALETTES[theme];
  if (!palette) throw new Error(`Unknown theme: ${theme}`);
  const lines = code.split('\n');
  if (
    tokens &&
    tokens
      .map((line) => line.map((token) => token.content).join(''))
      .join('\n') !== code
  )
    throw new Error('Shiki tokens must reconstruct exact source.');
  const FONT = 20;
  const CHAR = charWidth;
  const LINE = 32;
  const LANE = 35;
  const PAD = 36;
  const codeX = PAD + 14;
  const titleHeight = title ? 37 : 0;
  const starts: number[] = [];
  let offset = 0;
  for (const line of lines) {
    starts.push(offset);
    offset += line.length + 1;
  }
  const pos = (index: number) => {
    let line = starts.length - 1;
    while (line > 0 && starts[line] > index) line--;
    return { col: index - starts[line], line };
  };
  const resolved = annotations.map((annotation, i) => {
    if (!annotation.text || !annotation.label)
      throw new Error('Annotation text and label are required.');
    const occurrence = annotation.occurrence ?? 1;
    if (!Number.isInteger(occurrence) || occurrence < 1)
      throw new Error('Annotation occurrence must be a positive integer.');
    let start = -1;
    for (let n = 0; n < occurrence; n++) {
      start = code.indexOf(annotation.text, start + 1);
      if (start < 0)
        throw new Error(`Annotation text absent: ${annotation.text}`);
    }
    if (
      annotation.occurrence === undefined &&
      code.indexOf(annotation.text, start + 1) >= 0
    )
      throw new Error(`Ambiguous annotation: ${annotation.text}`);
    // A trailing line break should not annotate an empty next line.
    const significantEnd = start + annotation.text.replace(/\n+$/, '').length;
    return {
      ...annotation,
      a: pos(
        start + annotation.text.length - annotation.text.trimStart().length,
      ),
      b: pos(significantEnd),
      end: start + annotation.text.length,
      index: i,
      mark: annotation.mark ?? 'underbrace',
      start,
    };
  });
  const perLine = lines.map((_, i) =>
    resolved.filter((a) => a.b.line === i).sort((a, b) => b.a.col - a.a.col),
  );
  const lineY: number[] = [];
  let cursor = PAD + titleHeight + FONT;
  for (let i = 0; i < lines.length; i++) {
    lineY.push(cursor);
    cursor += LINE + perLine[i].length * LANE + (perLine[i].length ? 8 : 0);
  }
  const codeWidth = Math.max(...lines.map((l) => [...l].length)) * CHAR;
  let rightBound = codeX + codeWidth;
  const height = Math.ceil(cursor + PAD - LINE + 8);
  const renderedLines = lines
    .map((line, i) => {
      const spans = (tokens?.[i] ?? [{ color: palette.text, content: line }])
        .map(
          (t) =>
            `<tspan fill="${escapeXml(t.color ?? palette.text)}">${escapeXml(t.content)}</tspan>`,
        )
        .join('');
      return `<text x="${codeX}" y="${lineY[i]}" class="source-line">${spans}</text>`;
    })
    .join('\n');
  const underbrace = (x1: number, x2: number, y: number) => {
    const mid = (x1 + x2) / 2;
    const corner = Math.min(6, (x2 - x1) / 5);
    return `M ${x1} ${y} Q ${x1} ${y + 6} ${x1 + corner} ${y + 6} L ${mid - corner} ${y + 6} Q ${mid} ${y + 6} ${mid} ${y + 12} Q ${mid} ${y + 6} ${mid + corner} ${y + 6} L ${x2 - corner} ${y + 6} Q ${x2} ${y + 6} ${x2} ${y}`;
  };
  const annotationSvg = perLine
    .flatMap((items, line) =>
      items.map((a, lane) => {
        const color = palette[a.kind] ?? palette.syntax;
        const x1 = codeX + a.a.col * CHAR;
        const x2 = codeX + a.b.col * CHAR;
        const baseY = lineY[line] + 7;
        const labelY = baseY + 42 + lane * LANE;
        let mark: string;
        let leaderX: number;
        let leaderY: number;
        if (a.a.line !== a.b.line) {
          // A curly brace follows the outside edge of the code block.
          const blockWidth =
            Math.max(
              ...lines.slice(a.a.line, a.b.line + 1).map((l) => l.length),
            ) * CHAR;
          const right = codeX + blockWidth + 17;
          const top = lineY[a.a.line] - FONT + 3;
          const bottom = lineY[a.b.line] + 7;
          const mid = (top + bottom) / 2;
          mark = `<path d="M ${right} ${top} C ${right + 10} ${top}, ${right + 10} ${top + 8}, ${right + 10} ${top + 15} L ${right + 10} ${mid - 11} Q ${right + 10} ${mid} ${right + 20} ${mid} Q ${right + 10} ${mid} ${right + 10} ${mid + 11} L ${right + 10} ${bottom - 15} C ${right + 10} ${bottom - 8}, ${right + 10} ${bottom}, ${right} ${bottom}"/>`;
          leaderX = right + 20;
          leaderY = mid;
        } else if (a.mark === 'bracket') {
          mark = `<path d="M ${x1} ${baseY} V ${baseY + 8} H ${x2} V ${baseY}"/>`;
          leaderX = (x1 + x2) / 2;
          leaderY = baseY + 8;
        } else if (a.mark === 'circle') {
          const cx = (x1 + x2) / 2;
          const cy = lineY[line] - FONT / 2 + 3;
          const rx = Math.max((x2 - x1) / 2 + 4, 9);
          mark = `<ellipse cx="${cx}" cy="${cy}" rx="${rx}" ry="15"/>`;
          leaderX = cx;
          leaderY = cy + 15;
        } else {
          mark = `<path d="${underbrace(x1, x2, baseY)}"/>`;
          leaderX = (x1 + x2) / 2;
          leaderY = baseY + 12;
        }
        const isMultiline = a.a.line !== a.b.line;
        const labelX = leaderX + 60;
        // Rightmost targets use the upper label lanes. Curves only run down/right.
        const finalLabelY = isMultiline ? leaderY + 6 : labelY;
        const endY = finalLabelY - 5;
        const connector = `M ${leaderX} ${leaderY} C ${leaderX + 3} ${endY}, ${labelX - 30} ${endY}, ${labelX - 10} ${endY}`;
        rightBound = Math.max(rightBound, labelX + a.label.length * 8.6);
        return `<g class="annotation" data-kind="${escapeXml(a.kind)}" stroke="${color}">${mark}<path d="${connector}" stroke-opacity="0.78"/><text x="${labelX}" y="${finalLabelY}" fill="${color}" stroke="none" class="annotation-label">${escapeXml(a.label)}</text></g>`;
      }),
    )
    .join('\n');
  const width = Math.ceil(Math.max(rightBound, title.length * 8 + codeX) + PAD);
  const description = annotations
    .map((a) => `${a.label}: ${a.text}`)
    .join('. ');
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="snippet-title snippet-desc">
<title id="snippet-title">${escapeXml(title || 'Annotated code')}</title><desc id="snippet-desc">${escapeXml(description)}</desc>
<style>.source-line{font-family:${escapeXml(fontFamily)};font-size:${FONT}px;white-space:pre;font-variant-ligatures:none}.annotation{fill:none;stroke-width:1.7;stroke-linecap:round;stroke-linejoin:round}.annotation-label{font-family:Arial,sans-serif;font-size:16px;font-weight:400}</style>
<rect x="0.5" y="0.5" width="${width - 1}" height="${height - 1}" rx="12" fill="${palette.background}" stroke="${palette.border}"/>
${title ? `<text x="${codeX}" y="${PAD + 9}" fill="${palette.muted}" font-family="Arial,sans-serif" font-size="13" letter-spacing="1">${escapeXml(title)}</text>` : ''}
<g xml:space="preserve">${renderedLines}</g>
${annotationSvg}
</svg>`;
  return { height, svg, width };
}
