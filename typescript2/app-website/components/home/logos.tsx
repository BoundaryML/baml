import Image from 'next/image';
import { section } from './analytics';

const LOGOS: {
  name: string;
  src: string;
  h: number;
  w: number;
  light?: boolean;
}[] = [
  { h: 15, name: 'Stitch Fix', src: '/logos/stitchfix.png', w: 133 },
  { h: 19, light: true, name: 'Feather', src: '/logos/feather.png', w: 104 },
  { h: 28, name: 'SAP', src: '/logos/sap.svg', w: 57 },
  { h: 20, name: 'AMD', src: '/logos/amd.svg', w: 84 },
  { h: 31, name: 'EY', src: '/logos/ey.svg', w: 31 },
  { h: 27, name: 'Akamai', src: '/logos/akamai.svg', w: 66 },
  { h: 20, name: 'Vetcove', src: '/logos/vetcove.svg', w: 89 },
];

// Logos are drawn 1.2 times their listed size.
const SCALE = 1.2;
const LOGOS_WIDTH = LOGOS.reduce((sum, l) => sum + l.w * SCALE, 0);
// Each logo gets the same padding on both sides. Where there's room, the padding stretches so one row is
// exactly as wide as the strip: a logo leaving on the left is entering on the right, and the loop
// repeats with no visible seam. On narrower screens the padding stays at 2rem and the row overflows.
const PAD = `max(2rem, calc((100cqw - ${LOGOS_WIDTH}px) / ${LOGOS.length * 2}))`;

/** One run of the logos. The marquee draws two, the second hidden from screen readers. */
function Row({ copy }: { copy?: boolean }) {
  return (
    <ul
      aria-hidden={copy}
      className={`flex items-center shrink-0 ${copy ? 'v2-marquee-copy' : ''}`}
    >
      {LOGOS.map((l) => (
        <li key={l.name} style={{ paddingInline: PAD }}>
          <Image
            alt={copy ? '' : l.name}
            className={`${l.light ? 'logo-mark-light' : 'logo-mark'} max-w-none object-contain opacity-70`}
            height={l.h}
            src={l.src}
            style={{ height: l.h * SCALE, width: l.w * SCALE }}
            width={l.w}
          />
        </li>
      ))}
    </ul>
  );
}

/** The customer logo strip under the hero, scrolling between the page's grid rails. */
export default function Logos() {
  return (
    <section
      {...section('logos')}
      aria-label="Used by engineers at"
      className="border-y border-white/12 py-9 md:py-11"
    >
      <p className="mb-7 text-center text-[15px] text-white/60">
        Used by engineers at
      </p>
      {/* The strip spans rail to rail, like the section dividers, and is clipped there. */}
      <div className="mx-auto max-w-[1440px] px-(--frame-gutter)">
        <div className="v2-marquee @container">
          <div className="v2-marquee-track">
            <Row />
            <Row copy />
          </div>
        </div>
      </div>
    </section>
  );
}
