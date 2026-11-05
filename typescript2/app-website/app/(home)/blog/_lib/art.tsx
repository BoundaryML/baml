import type { ReactNode } from 'react';
import type { Post } from './get-posts';

// A drawn preview for a post on the blog index, in place of its share card: a small diagram of the
// post's point, such as messy code beside the clean version. Each is an SVG on a 1200 by 630 canvas
// (the share card's shape), so it scales to the lead slot or a card. Posts without one fall back to
// their first image, then to a plain pattern.

const W = 1200;
const H = 630;
const PAD = 48;
// Ligatures off, so `=>` and `!=` show as typed.
const MONO = { fontFamily: 'var(--font-mono)', fontVariantLigatures: 'none' };
const SANS = { fontFamily: 'var(--font-sans)' };
const DISPLAY = { fontFamily: 'var(--font-display)' };
// A soft red for what a post shows going wrong. It is only used beside words that say so.
const WRONG = '#f0857a';

const KEYWORDS =
  /("[^"]*"|\/\/.*$|```\w*|\b(?:function|let|if|else|while|for|in|match|enum|return|break)\b)/;

/** One line of code, colored: keywords in the accent, strings in white, comments muted. */
function CodeLine({
  size,
  text,
  tone = 'var(--body)',
  x,
  y,
}: {
  size: number;
  text: string;
  tone?: string;
  x: number;
  y: number;
}) {
  const parts = text.split(new RegExp(KEYWORDS.source, 'g'));
  return (
    <text
      fill={tone}
      fontSize={size}
      style={{ ...MONO, whiteSpace: 'pre' }}
      x={x}
      xmlSpace="preserve"
      y={y}
    >
      {parts.map((part, i) => {
        if (i % 2 === 0) return part;
        const fill = part.startsWith('"')
          ? 'var(--head)'
          : part.startsWith('//') || part.startsWith('```')
            ? 'var(--muted)'
            : 'var(--color-clover)';
        return (
          // biome-ignore lint/suspicious/noArrayIndexKey: a line's tokens never reorder.
          <tspan fill={fill} key={i}>
            {part}
          </tspan>
        );
      })}
    </text>
  );
}

/** A dark pane with a title, its contents clipped to it, and a fade where long lines run off its
 *  right edge. */
function Pane({
  children,
  id,
  title,
  w,
  x,
}: {
  children: ReactNode;
  id: string;
  title: string;
  w: number;
  x: number;
}) {
  const h = H - PAD * 2;
  return (
    <g>
      <defs>
        <clipPath id={`${id}-clip`}>
          <rect height={h} rx={20} width={w} x={x} y={PAD} />
        </clipPath>
        <linearGradient id={`${id}-fade`} x1="0" x2="1" y1="0" y2="0">
          <stop offset="0" stopColor="var(--ground)" stopOpacity="0" />
          <stop offset="1" stopColor="var(--ground)" />
        </linearGradient>
      </defs>
      <rect
        fill="var(--ground)"
        height={h}
        rx={20}
        stroke="var(--rule)"
        strokeWidth={2}
        width={w}
        x={x}
        y={PAD}
      />
      <g clipPath={`url(#${id}-clip)`}>
        {children}
        <rect
          fill={`url(#${id}-fade)`}
          height={h - 140}
          width={64}
          x={x + w - 64}
          y={PAD + 70}
        />
      </g>
      <text
        fill="var(--muted)"
        fontSize={22}
        style={SANS}
        x={x + 32}
        y={PAD + 50}
      >
        {title}
      </text>
    </g>
  );
}

/** Code lines in a pane, from its top, with faint guides at each level of indentation. */
function Code({
  guides = false,
  lines,
  size = 17,
  x,
  y = PAD + 104,
}: {
  guides?: boolean;
  lines: { text: string; tone?: string }[];
  size?: number;
  x: number;
  y?: number;
}) {
  const lh = size * 1.55;
  const ch = size * 0.6;
  return (
    <g>
      {guides &&
        lines.flatMap((line, i) => {
          const depth = Math.floor(
            (line.text.length - line.text.trimStart().length) / 4,
          );
          return Array.from({ length: depth }, (_, k) => (
            <line
              // biome-ignore lint/suspicious/noArrayIndexKey: guides are fixed per line.
              key={`${i}-${k}`}
              stroke="var(--rule)"
              strokeWidth={2}
              x1={x + k * 4 * ch + ch / 2}
              x2={x + k * 4 * ch + ch / 2}
              y1={y + (i - 1) * lh + size * 0.35}
              y2={y + i * lh + size * 0.35}
            />
          ));
        })}
      {lines.map((line, i) => (
        <CodeLine
          // biome-ignore lint/suspicious/noArrayIndexKey: code lines never reorder.
          key={i}
          size={size}
          text={line.text}
          tone={line.tone}
          x={x}
          y={y + i * lh}
        />
      ))}
    </g>
  );
}

/** A label and a large value at the bottom of a pane. */
function Stat({
  color = 'var(--head)',
  label,
  value,
  w,
  x,
}: {
  color?: string;
  label: string;
  value: string;
  w: number;
  x: number;
}) {
  return (
    <g>
      <line
        stroke="var(--rule)"
        strokeWidth={2}
        x1={x}
        x2={x + w}
        y1={H - PAD - 96}
        y2={H - PAD - 96}
      />
      <text
        fill="var(--muted)"
        fontSize={22}
        style={SANS}
        x={x + 32}
        y={H - PAD - 38}
      >
        {label}
      </text>
      <text
        fill={color}
        fontSize={60}
        style={{ ...DISPLAY, fontWeight: 500 }}
        textAnchor="end"
        x={x + w - 32}
        y={H - PAD - 30}
      >
        {value}
      </text>
    </g>
  );
}

const HALF = (W - PAD * 2 - 36) / 2;
const LEFT = PAD;
const RIGHT = PAD + HALF + 36;

/** "Teaching Agents to Write Beautiful BAML": the agent's chess move generator before and after the
 *  new skill, with each function's cognitive complexity. */
function BeautifulBaml({ id }: { id: string }) {
  return (
    <>
      <Pane id={`${id}-a`} title="Before" w={HALF} x={LEFT}>
        <Code
          guides
          lines={[
            'function chess_candidates(position: ChessPosition) -> ChessCandidate[] {',
            '    while (from < 64) {',
            '        let piece = board[from];',
            '        if (chess_color(piece, position.white)) {',
            '            let kind = piece.to_lower_case();',
            '            if (kind == "p") {',
            '                chess_pawn_candidates(position, from, candidates);',
            '            } else {',
            '                for (let direction in directions) {',
            '                    if ((kind != "b" || diagonal) && (kind != "r" || !diagonal)) {',
            '                        while (chess_inside(x, y)) {',
          ].map((text) => ({ text }))}
          x={LEFT + 32}
        />
        <Stat label="Cognitive complexity" value="50" w={HALF} x={LEFT} />
      </Pane>
      <Pane id={`${id}-b`} title="After" w={HALF} x={RIGHT}>
        <Code
          lines={[
            'function candidates(self, from: int) -> ChessPly[] {',
            '    let piece = self.board[from] ?? return [];',
            '    match (piece.kind) {',
            '        ChessKind.Pawn => self.pawn_moves(from),',
            '        ChessKind.Knight => CHESS_KNIGHT_STEPS.flat_map(…),',
            '        ChessKind.Bishop => CHESS_DIAGONALS.flat_map(…),',
            '        ChessKind.Rook => CHESS_STRAIGHTS.flat_map(…),',
            '        ChessKind.Queen => CHESS_RAYS.flat_map(…),',
            '        ChessKind.King => CHESS_RAYS.flat_map(…),',
            '    }',
            '}',
          ].map((text) => ({ text }))}
          x={RIGHT + 32}
        />
        <Stat
          color="var(--color-clover)"
          label="Cognitive complexity"
          value="2"
          w={HALF}
          x={RIGHT}
        />
      </Pane>
    </>
  );
}

/** "Prompting vs JSON Mode vs ... SAP": a model's sloppy answer, which isn't JSON, and the typed value
 *  schema-aligned parsing reads out of it. The lines that break JSON are marked. */
function SchemaAlignedParsing({ id }: { id: string }) {
  const raw = [
    { bad: true, text: "Sure! Here's the resume:" },
    { bad: true, text: '```json' },
    { text: '{' },
    { bad: true, text: '  name: "Ada Lovelace",' },
    { bad: true, text: '  // most recent first' },
    { text: '  experience: [' },
    { bad: true, text: '    "Analytical Engine",' },
    { text: '  ],' },
    { text: '}' },
  ];
  const y0 = PAD + 104;
  const lh = 17 * 1.55;
  return (
    <>
      <Pane id={`${id}-a`} title="Model output" w={HALF} x={LEFT}>
        {raw.map(
          (line, i) =>
            line.bad && (
              <circle
                cx={LEFT + 30}
                cy={y0 + i * lh - 6}
                fill={WRONG}
                // biome-ignore lint/suspicious/noArrayIndexKey: fixed lines.
                key={i}
                r={4}
              />
            ),
        )}
        <Code lines={raw} x={LEFT + 48} />
        <Stat
          color={WRONG}
          label="JSON.parse"
          value="Fails"
          w={HALF}
          x={LEFT}
        />
      </Pane>
      <Pane id={`${id}-b`} title="Parsed with SAP" w={HALF} x={RIGHT}>
        <Code
          lines={[
            'Resume {',
            '  name: "Ada Lovelace",',
            '  experience: [',
            '    "Analytical Engine",',
            '  ],',
            '}',
          ].map((text) => ({ text }))}
          x={RIGHT + 48}
        />
        <Stat
          color="var(--color-clover)"
          label="Type"
          value="Resume"
          w={HALF}
          x={RIGHT}
        />
      </Pane>
    </>
  );
}

/** "Structured Outputs Create False Confidence": the same receipt read by the structured outputs API
 *  and by plain text output that is then parsed. Only the second gets the bananas' quantity right. */
function FalseConfidence({ id }: { id: string }) {
  const item = (price: string, quantity: string) =>
    [
      '"items": [',
      '  {',
      '    "name": "Bananas",',
      `    "price": ${price},`,
      `    "quantity": ${quantity}`,
      '  }',
      ']',
    ].map((text) => ({ text }));
  return (
    <>
      <Pane id={`${id}-a`} title="Structured outputs API" w={HALF} x={LEFT}>
        <Code lines={item('0.32', '1')} x={LEFT + 32} />
        <Stat
          color={WRONG}
          label="Quantity, wrong"
          value="1"
          w={HALF}
          x={LEFT}
        />
      </Pane>
      <Pane id={`${id}-b`} title="Text output, then parsed" w={HALF} x={RIGHT}>
        <Code lines={item('0.69', '0.46')} x={RIGHT + 32} />
        <Stat
          color="var(--color-clover)"
          label="Quantity, right"
          value="0.46"
          w={HALF}
          x={RIGHT}
        />
      </Pane>
    </>
  );
}

const FRAMEWORKS = [
  'BAML',
  'Instructor',
  'TypeChat',
  'Marvin',
  'Outlines',
  'Guidance',
  'LMQL',
  'JSONformer',
  'Genkit',
  'SGLang',
];
const CRITERIA = [
  'Malformed JSON',
  'Prompt control',
  'Providers',
  'Output types',
  'Tests',
];

/** "Every Way To Get Structured Output From LLMs": the survey's comparison table, sketched, with its
 *  frameworks down the side and its questions across the top. */
function ComparisonChart({ id }: { id: string }) {
  const w = W - PAD * 2;
  const nameW = 230;
  const colW = (w - 64 - nameW) / CRITERIA.length;
  const top = PAD + 92;
  const rowH = 40;
  return (
    <Pane id={id} title="Structured output frameworks, compared" w={w} x={PAD}>
      {CRITERIA.map((c, j) => (
        <text
          fill="var(--body)"
          fontSize={19}
          key={c}
          style={SANS}
          x={PAD + 32 + nameW + j * colW}
          y={top}
        >
          {c}
        </text>
      ))}
      {FRAMEWORKS.map((name, i) => {
        const y = top + 22 + i * rowH;
        return (
          <g key={name}>
            <line
              stroke="var(--rule)"
              strokeWidth={1.5}
              x1={PAD + 32}
              x2={PAD + w - 32}
              y1={y}
              y2={y}
            />
            <text
              fill="var(--head)"
              fontSize={19}
              style={MONO}
              x={PAD + 32}
              y={y + 27}
            >
              {name}
            </text>
            {/* Each cell is a placeholder line of text, of a length that varies to look like a
                written answer. They carry no data. */}
            {CRITERIA.map((c, j) => (
              <rect
                fill="var(--rule)"
                height={8}
                key={c}
                rx={4}
                width={60 + ((i * 7 + j * 13) % 5) * 18}
                x={PAD + 32 + nameW + j * colW}
                y={y + 16}
              />
            ))}
          </g>
        );
      })}
    </Pane>
  );
}

/** "1 co-founder, 5 years, 12 pivots": a winding path with a stop at each pivot, ending at the 12th,
 *  BAML. */
function Pivots({ id }: { id: string }) {
  const stops: [number, number][] = [
    [130, 470],
    [220, 300],
    [300, 430],
    [390, 220],
    [470, 360],
    [550, 260],
    [620, 440],
    [700, 300],
    [790, 400],
    [860, 200],
    [930, 330],
    [1000, 250],
  ];
  const d = stops.map(([x, y], i) => `${i ? 'L' : 'M'} ${x} ${y}`).join(' ');
  const w = W - PAD * 2;
  const [lastX, lastY] = stops.at(-1) ?? stops[0];
  return (
    <Pane id={id} title="12 pivots in 5 years" w={w} x={PAD}>
      <path
        d={d}
        fill="none"
        stroke="var(--rule)"
        strokeLinejoin="round"
        strokeWidth={4}
      />
      {stops.map(([x, y], i) => {
        const end = i === stops.length - 1;
        return (
          <g key={`${x}-${y}`}>
            <circle
              cx={x}
              cy={y}
              fill={end ? 'var(--color-clover)' : 'var(--ground)'}
              r={end ? 20 : 15}
              stroke={end ? 'var(--color-clover)' : 'var(--muted)'}
              strokeWidth={2}
            />
            <text
              fill={end ? 'var(--ground)' : 'var(--body)'}
              fontSize={end ? 18 : 15}
              style={{ ...MONO, fontWeight: end ? 600 : 400 }}
              textAnchor="middle"
              x={x}
              y={y + (end ? 6 : 5)}
            >
              {i + 1}
            </text>
          </g>
        );
      })}
      <text
        fill="var(--color-clover)"
        fontSize={44}
        style={{ ...DISPLAY, fontWeight: 500 }}
        textAnchor="middle"
        x={lastX}
        y={lastY - 40}
      >
        BAML
      </text>
    </Pane>
  );
}

const ART: Record<string, (props: { id: string }) => ReactNode> = {
  '5-years-12-pivots': Pivots,
  'schema-aligned-parsing': SchemaAlignedParsing,
  'structured-output-from-llms': ComparisonChart,
  'structured-outputs-create-false-confidence': FalseConfidence,
  'teaching-agents-to-write-beautiful-baml': BeautifulBaml,
};

/** Whether a post has a drawn preview. */
export const hasArt = (post: Post) => post.slug in ART;

/** A post's preview at the share card's shape: its drawing, else its first image, else a plain
 *  pattern. `id` keeps the drawing's clip paths apart when one post's preview shows twice. */
export function PostArt({
  className = '',
  id,
  post,
}: {
  className?: string;
  id: string;
  post: Post;
}) {
  const box = `w-full aspect-[1200/630] overflow-hidden bg-raised ${className}`;
  const Art = ART[post.slug];
  if (Art) {
    return (
      <svg
        aria-hidden
        className={box}
        viewBox={`0 0 ${W} ${H}`}
        xmlns="http://www.w3.org/2000/svg"
      >
        <title>{post.title}</title>
        <Art id={id} />
      </svg>
    );
  }
  if (post.firstImage) {
    return (
      // biome-ignore lint/performance/noImgElement: a post's first image is any file from its body, with no known size.
      <img alt="" className={`${box} object-cover`} src={post.firstImage} />
    );
  }
  return (
    <span
      aria-hidden
      className={`${box} block bg-[radial-gradient(var(--rule)_1.5px,transparent_1.5px)] bg-size-[22px_22px]`}
    />
  );
}
