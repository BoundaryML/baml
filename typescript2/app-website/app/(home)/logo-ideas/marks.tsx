import { type ReactNode, type SVGProps, useId } from 'react';
import { ALT_LOGO, ALT_MARK } from '@/components/home/boundary-alt';
import { BAML_LOGO } from '@/components/home/lamb';

// Logo ideas for Boundary, BAML and the two together, drawn for the /logo-ideas review page. Every
// mark sits in a 120 by 120 box. `mono` draws it flat in currentColor; otherwise it uses the lane
// gradients below.

export type MarkProps = SVGProps<SVGSVGElement> & { mono?: boolean };

// Pasture for Boundary, Clover for BAML, each from a light step to a mid step of its scale.
const GREEN = ['#E1F3DC', '#82B571', '#4A7A39'];
const PURPLE = ['#F1E9FF', '#CAAAF2', '#9A5BDA'];

function Svg({
  children,
  label,
  ...rest
}: SVGProps<SVGSVGElement> & { children: ReactNode; label: string }) {
  return (
    <svg fill="currentColor" role="img" viewBox="0 0 120 120" {...rest}>
      <title>{label}</title>
      {children}
    </svg>
  );
}

// A top-left to bottom-right gradient in the mark's own coordinates.
function Ramp({
  colors,
  id,
  x1 = 20,
  x2 = 100,
  y1 = 0,
  y2 = 120,
}: {
  colors: string[];
  id: string;
  x1?: number;
  x2?: number;
  y1?: number;
  y2?: number;
}) {
  return (
    <linearGradient
      gradientUnits="userSpaceOnUse"
      id={id}
      x1={x1}
      x2={x2}
      y1={y1}
      y2={y2}
    >
      {colors.map((c, i) => (
        <stop key={c} offset={i / (colors.length - 1)} stopColor={c} />
      ))}
    </linearGradient>
  );
}

/* ---------------------------------- Boundary ---------------------------------- */

// A disc cut into three slanted bands that fade from top to bottom.
export function Slant({ mono, ...rest }: MarkProps) {
  const id = useId();
  const ink = mono ? 'currentColor' : `url(#${id}g)`;
  return (
    <Svg label="Slant" {...rest}>
      <defs>
        <Ramp colors={GREEN} id={`${id}g`} />
        <clipPath id={`${id}c`}>
          <circle cx="60" cy="60" r="52" />
        </clipPath>
      </defs>
      <g clipPath={`url(#${id}c)`}>
        <g fill={ink} transform="rotate(-17 60 60)">
          <rect height="50" width="160" x="-20" y="-12" />
          <rect height="34" opacity="0.65" width="160" x="-20" y="45" />
          <rect height="50" opacity="0.35" width="160" x="-20" y="86" />
        </g>
      </g>
    </Svg>
  );
}

// A geometric lowercase b: a round-ended post and a ring that share one edge.
export function RingB({ mono, ...rest }: MarkProps) {
  const id = useId();
  const ink = mono ? 'currentColor' : `url(#${id})`;
  return (
    <Svg label="Ring b" {...rest}>
      <defs>
        <Ramp colors={GREEN} id={id} />
      </defs>
      <g fill="none" stroke={ink} strokeWidth="17">
        <path d="M30.5 16.5V76" strokeLinecap="round" />
        <circle cx="60" cy="76" r="29.5" />
      </g>
    </Svg>
  );
}

// A disc whose lower half thins out into rows, like a field running to the horizon.
export function Field({ mono, ...rest }: MarkProps) {
  const id = useId();
  const ink = mono ? 'currentColor' : `url(#${id}g)`;
  return (
    <Svg label="Field" {...rest}>
      <defs>
        <Ramp colors={GREEN} id={`${id}g`} x1={60} x2={60} y1={8} y2={112} />
        <clipPath id={`${id}c`}>
          <circle cx="60" cy="60" r="52" />
        </clipPath>
      </defs>
      <g clipPath={`url(#${id}c)`} fill={ink}>
        <rect height="54" width="120" x="0" y="8" />
        <rect height="10" width="120" x="0" y="66" />
        <rect height="8" width="120" x="0" y="81" />
        <rect height="6" width="120" x="0" y="95" />
        <rect height="4" width="120" x="0" y="107" />
      </g>
    </Svg>
  );
}

// The alt mark, lit: each bar gets its own gradient so the mark reads as a folded ribbon.
export function LitAlt({ mono, ...rest }: MarkProps) {
  const id = useId();
  return (
    <Svg label="Lit alt mark" {...rest}>
      <defs>
        <Ramp
          colors={['#FFFFFF', '#C7E5BD']}
          id={`${id}a`}
          x1={0}
          x2={86}
          y1={0}
          y2={50}
        />
        <Ramp
          colors={['#5F934E', '#3E6B2E']}
          id={`${id}b`}
          x1={0}
          x2={46}
          y1={40}
          y2={68}
        />
        <Ramp
          colors={['#C7E5BD', '#5F934E']}
          id={`${id}c`}
          x1={0}
          x2={86}
          y1={52}
          y2={144}
        />
      </defs>
      <g transform="translate(25.6 2.4) scale(0.8)">
        <path
          d={ALT_MARK.paths[1]}
          fill={mono ? 'currentColor' : `url(#${id}a)`}
        />
        <path
          d={ALT_MARK.paths[0]}
          fill={mono ? 'currentColor' : `url(#${id}b)`}
        />
        <path
          d={ALT_MARK.paths[2]}
          fill={mono ? 'currentColor' : `url(#${id}c)`}
        />
      </g>
    </Svg>
  );
}

/* ------------------------------------ BAML ------------------------------------ */

// A lowercase b whose bowl winds inward like a ram's horn.
export function Horn({ mono, ...rest }: MarkProps) {
  const id = useId();
  return (
    <Svg label="Horn" {...rest}>
      <defs>
        <Ramp colors={PURPLE} id={id} />
      </defs>
      <path
        d="M30 12V74A34 34 0 0 0 98 74A26 26 0 0 0 46 74A18 18 0 0 0 82 74A10 10 0 0 0 62 74"
        fill="none"
        stroke={mono ? 'currentColor' : `url(#${id})`}
        strokeLinecap="round"
        strokeWidth="10"
      />
    </Svg>
  );
}

// A lamb's head from circles and arcs: three circles of wool, two ears, a rounded face.
export function LambHead({ mono, ...rest }: MarkProps) {
  const id = useId();
  const wool = (
    <>
      <circle cx="40" cy="42" r="16" />
      <circle cx="60" cy="32" r="19" />
      <circle cx="80" cy="42" r="16" />
    </>
  );
  return (
    <Svg label="Lamb head" {...rest}>
      <defs>
        <Ramp colors={[PURPLE[1], PURPLE[2]]} id={`${id}f`} y1={50} />
        <Ramp colors={['#FFFFFF', PURPLE[0]]} id={`${id}w`} y2={60} />
        <mask id={`${id}m`}>
          <rect fill="#fff" height="120" width="120" />
          <g fill="#000" stroke="#000" strokeWidth="7">
            {wool}
          </g>
          <circle cx="50" cy="76" fill="#000" r="4.5" />
          <circle cx="70" cy="76" fill="#000" r="4.5" />
        </mask>
      </defs>
      <g fill={mono ? 'currentColor' : `url(#${id}f)`} mask={`url(#${id}m)`}>
        <ellipse
          cx="21"
          cy="64"
          rx="15"
          ry="7.5"
          transform="rotate(20 21 64)"
        />
        <ellipse
          cx="99"
          cy="64"
          rx="15"
          ry="7.5"
          transform="rotate(-20 99 64)"
        />
        <path d="M38 50H82V84A22 22 0 0 1 38 84Z" />
      </g>
      <g fill={mono ? 'currentColor' : `url(#${id}w)`}>{wool}</g>
    </Svg>
  );
}

// Four overlapping translucent discs: a four-leaf clover, after BAML's color.
export function Clover({ mono, ...rest }: MarkProps) {
  const id = useId();
  const ink = mono ? 'currentColor' : `url(#${id})`;
  return (
    <Svg label="Clover" {...rest}>
      <defs>
        <Ramp colors={PURPLE} id={id} />
      </defs>
      <g fill={ink} opacity={mono ? 1 : undefined}>
        <circle cx="41" cy="41" fillOpacity="0.72" r="27" />
        <circle cx="79" cy="41" fillOpacity="0.72" r="27" />
        <circle cx="41" cy="79" fillOpacity="0.72" r="27" />
        <circle cx="79" cy="79" fillOpacity="0.72" r="27" />
      </g>
    </Svg>
  );
}

// A solid lowercase b with a prompt chevron cut out as its counter.
export function PromptB({ mono, ...rest }: MarkProps) {
  const id = useId();
  return (
    <Svg label="Prompt b" {...rest}>
      <defs>
        <Ramp colors={PURPLE} id={`${id}g`} />
        <mask id={`${id}m`}>
          <rect fill="#fff" height="120" width="120" />
          <path
            d="M56 54L76 72L56 90"
            fill="none"
            stroke="#000"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="11"
          />
        </mask>
      </defs>
      <g fill={mono ? 'currentColor' : `url(#${id}g)`} mask={`url(#${id}m)`}>
        <rect height="104" rx="5" width="24" x="16" y="8" />
        <circle cx="66" cy="72" r="40" />
      </g>
    </Svg>
  );
}

/* ---------------------------------- Combined ---------------------------------- */

// An arc over a post, with a dot at its head: the lamb's leap over the boundary, abstracted.
export function Leap({ mono, ...rest }: MarkProps) {
  const id = useId();
  return (
    <Svg label="Leap" {...rest}>
      <defs>
        <linearGradient
          gradientUnits="userSpaceOnUse"
          id={`${id}a`}
          x1="14"
          x2="80"
          y1="100"
          y2="30"
        >
          <stop offset="0" stopColor={PURPLE[2]} stopOpacity="0.1" />
          <stop offset="1" stopColor={PURPLE[1]} />
        </linearGradient>
        <linearGradient id={`${id}m`} x1="0" x2="1" y1="1" y2="0">
          <stop offset="0" stopColor="currentColor" stopOpacity="0.15" />
          <stop offset="1" stopColor="currentColor" />
        </linearGradient>
        <Ramp
          colors={[GREEN[1], GREEN[2]]}
          id={`${id}p`}
          x1={60}
          x2={60}
          y1={56}
          y2={112}
        />
      </defs>
      <path
        d="M14 100Q51.7 -14.8 89.4 58.7"
        fill="none"
        stroke={mono ? `url(#${id}m)` : `url(#${id}a)`}
        strokeLinecap="round"
        strokeWidth="10"
      />
      <circle
        cx="89.4"
        cy="58.7"
        fill={mono ? 'currentColor' : PURPLE[0]}
        r="11"
      />
      <rect
        fill={mono ? 'currentColor' : `url(#${id}p)`}
        height="48"
        rx="6"
        width="12"
        x="54"
        y="66"
      />
    </Svg>
  );
}

// Two lowercase b's linked through each other: one for Boundary, one for BAML.
export function DoubleB({ mono, ...rest }: MarkProps) {
  const id = useId();
  const green = mono ? 'currentColor' : `url(#${id}g)`;
  const purple = mono ? 'currentColor' : `url(#${id}p)`;
  return (
    <Svg label="Double b" {...rest}>
      <defs>
        <Ramp colors={GREEN} id={`${id}g`} x1={16} x2={70} y1={10} y2={90} />
        <Ramp colors={PURPLE} id={`${id}p`} x1={46} x2={106} y1={30} y2={112} />
      </defs>
      <g fill="none" strokeLinecap="round" strokeWidth="11">
        <path d="M22 12V66" stroke={green} />
        <circle cx="46" cy="66" r="24" stroke={green} />
        <path d="M52 30V84" opacity={mono ? 0.6 : 1} stroke={purple} />
        <circle
          cx="76"
          cy="84"
          opacity={mono ? 0.6 : 1}
          r="24"
          stroke={purple}
        />
      </g>
    </Svg>
  );
}

// The Horn b in two colors: Boundary's post and BAML's horn.
export function PostAndHorn({ mono, ...rest }: MarkProps) {
  const id = useId();
  return (
    <Svg label="Post and horn" {...rest}>
      <defs>
        <Ramp colors={GREEN} id={`${id}g`} x1={30} x2={30} y1={10} y2={80} />
        <Ramp colors={PURPLE} id={`${id}p`} y1={40} />
      </defs>
      <g fill="none" strokeLinecap="round" strokeWidth="10">
        <path
          d="M30 74A34 34 0 0 0 98 74A26 26 0 0 0 46 74A18 18 0 0 0 82 74A10 10 0 0 0 62 74"
          opacity={mono ? 0.6 : 1}
          stroke={mono ? 'currentColor' : `url(#${id}p)`}
        />
        <path d="M30 12V74" stroke={mono ? 'currentColor' : `url(#${id}g)`} />
      </g>
    </Svg>
  );
}

// A disc split by one slanted cut: Boundary's half above, BAML's half below.
export function Split({ mono, ...rest }: MarkProps) {
  const id = useId();
  return (
    <Svg label="Split" {...rest}>
      <defs>
        <Ramp colors={GREEN} id={`${id}g`} y2={70} />
        <Ramp colors={[PURPLE[1], PURPLE[2]]} id={`${id}p`} y1={50} />
        <clipPath id={`${id}c`}>
          <circle cx="60" cy="60" r="52" />
        </clipPath>
      </defs>
      <g clipPath={`url(#${id}c)`}>
        <g transform="rotate(-17 60 60)">
          <rect
            fill={mono ? 'currentColor' : `url(#${id}g)`}
            height="66"
            width="160"
            x="-20"
            y="-10"
          />
          <rect
            fill={mono ? 'currentColor' : `url(#${id}p)`}
            height="66"
            opacity={mono ? 0.55 : 1}
            width="160"
            x="-20"
            y="64"
          />
        </g>
      </g>
    </Svg>
  );
}

/* ----------------------------------- Lockups ----------------------------------- */

// The alt wordmark's letters without its mark. They span x 116.7 to 610 and y 29.3 to 137.5.
const BOUNDARY_LETTERS = ALT_LOGO.paths.slice(ALT_MARK.paths.length);
// The BAML wordmark's letters without its lamb. They span x 0 to 386 and y 94.45 to 223.93.
const BAML_LETTERS = BAML_LOGO.paths.slice(0, 3);

type MarkComponent = (props: MarkProps) => ReactNode;

// A mark with a wordmark beside it, the wordmark in currentColor.
export function Lockup({
  className,
  Mark,
  mono,
  word,
}: {
  className?: string;
  Mark: MarkComponent;
  mono?: boolean;
  word: 'boundary' | 'baml';
}) {
  // Both wordmarks sit on a baseline at y 100 with letters about 70 tall.
  const k = word === 'boundary' ? 0.85 : 0.54;
  const baseline = word === 'boundary' ? 111.7 : 223.93;
  const left = word === 'boundary' ? 116.7 : 0;
  const span = word === 'boundary' ? 493.3 : 386;
  const letters = word === 'boundary' ? BOUNDARY_LETTERS : BAML_LETTERS;
  const width = 150 + span * k + 2;
  return (
    <svg
      className={className}
      fill="currentColor"
      role="img"
      viewBox={`0 0 ${width} 124`}
    >
      <title>{word}</title>
      <Mark height={120} mono={mono} width={120} x={0} y={0} />
      <g
        transform={`translate(${150 - left * k} ${100 - baseline * k}) scale(${k})`}
      >
        {letters.map((d) => (
          <path d={d} key={d} />
        ))}
      </g>
    </svg>
  );
}
