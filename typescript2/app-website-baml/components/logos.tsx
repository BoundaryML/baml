import Image from 'next/image';
import { section } from './analytics';
import { WRAP } from './ui';

/* Used by engineers at: one row of customer marks under the hero, wrapping on a phone. Heights are
   per logo so each takes about the same visual area; widths follow from each file's aspect ratio.
   The marks are greyed and dimmed until hovered (.logo-mark in globals.css), so seven brand colours
   don't compete with the hero. */
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

export default function Logos() {
  return (
    <section
      {...section('logos')}
      aria-label="Used by engineers at"
      className={`${WRAP} pt-4 pb-10 md:pb-12`}
    >
      <div className="flex flex-col items-center gap-6 lg:flex-row lg:gap-12">
        <p className="shrink-0 text-[15px] text-muted">Used by engineers at</p>
        <ul className="flex flex-1 flex-wrap items-center justify-center gap-x-10 gap-y-6 lg:justify-between">
          {LOGOS.map((l) => (
            <li key={l.name}>
              <Image
                alt={l.name}
                className={`${l.light ? 'logo-mark-light' : 'logo-mark'} w-auto opacity-60 transition-opacity duration-300 hover:opacity-100`}
                height={l.h}
                src={l.src}
                style={{ height: l.h }}
                title={l.name}
                width={l.w}
              />
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
